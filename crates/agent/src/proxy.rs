//! The secret proxy: commands use API keys without the key ever entering
//! their process.
//!
//! For a known key (OpenAI, Anthropic, or a custom profile from the vault),
//! the command gets a placeholder key
//! and a base URL pointing at this proxy on 127.0.0.1. The proxy strips any
//! auth the command sends, adds the real key, and forwards to the provider's
//! one fixed upstream over HTTPS, streaming the response back (SSE included).
//! A prompt-injected agent has nothing to leak; it can still make API calls
//! while the run lasts.
//!
//! Each run gets its own route under a random 128-bit path token, removed
//! when the run ends, so other local users can't borrow it over loopback.
//! It's not an open proxy: every route has exactly one upstream.

use std::collections::HashMap;
use std::io::Read;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tiny_http::{Header, Response, Server, StatusCode};
use zeroize::Zeroizing;

/// What the command sees instead of the real key.
pub const PLACEHOLDER: &str = "dotsafe-proxied";
/// Requests to providers are small; anything bigger is refused.
const MAX_REQUEST_BODY: u64 = 32 * 1024 * 1024;

/// A provider whose SDKs read a base URL from the environment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Profile {
    pub name: String,
    /// The variable holding the key.
    pub key_var: String,
    /// The variable SDKs read the base URL from.
    pub base_url_var: String,
    pub upstream: String,
    /// Appended to the proxy URL in `base_url_var` (what SDKs expect there).
    pub base_path: String,
    pub header: String,
    /// Send the key as `Bearer <key>` rather than as is.
    pub bearer: bool,
}

/// The profiles dotsafe knows without being told.
pub fn builtin_profiles() -> Vec<Profile> {
    let profile = |name: &str, key: &str, base: &str, upstream: &str, path: &str, header: &str, bearer| Profile {
        name: name.into(),
        key_var: key.into(),
        base_url_var: base.into(),
        upstream: upstream.into(),
        base_path: path.into(),
        header: header.into(),
        bearer,
    };
    vec![
        profile("OpenAI", "OPENAI_API_KEY", "OPENAI_BASE_URL", "https://api.openai.com", "/v1", "authorization", true),
        profile("Anthropic", "ANTHROPIC_API_KEY", "ANTHROPIC_BASE_URL", "https://api.anthropic.com", "", "x-api-key", false),
    ]
}

/// Headers a custom profile may not use: they'd break or confuse requests.
const RESERVED_HEADERS: &[&str] =
    &["host", "content-length", "transfer-encoding", "connection", "cookie", "proxy-authorization"];

impl Profile {
    /// A custom profile from the vault, if it's safe to use: an `https://`
    /// upstream with a plain host (no credentials, query or fragment), valid
    /// variable names, a plain header name, and no built-in variable taken
    /// over. Returns why not otherwise.
    pub fn custom(record: &dotsafe_core::ProxyProfile) -> Result<Profile, String> {
        let name = record.name.trim();
        if name.is_empty() {
            return Err("Give the profile a name.".into());
        }
        for var in [&record.key_var, &record.base_url_var] {
            if !dotsafe_core::is_valid_var_name(var) {
                return Err(format!("{var:?} isn't a valid variable name."));
            }
        }
        if record.key_var == record.base_url_var {
            return Err("The key and base URL variables must differ.".into());
        }
        if let Some(builtin) = builtin_profiles()
            .into_iter()
            .find(|p| [&p.key_var, &p.base_url_var].iter().any(|v| **v == record.key_var || **v == record.base_url_var))
        {
            return Err(format!("{} already goes through the proxy as {}.", record.key_var, builtin.name));
        }
        let upstream = record.upstream.trim().trim_end_matches('/');
        let host = upstream.strip_prefix("https://").ok_or("The upstream must start with https://.")?;
        let host_part = host.split('/').next().unwrap_or_default();
        if host_part.is_empty() || host.contains(['@', '?', '#', ' ']) || host_part.contains('\\') {
            return Err("The upstream must be a plain https:// address, like https://api.example.com.".into());
        }
        if !record.base_path.is_empty() && (!record.base_path.starts_with('/') || record.base_path.contains(['?', '#', ' '])) {
            return Err("The base path must start with /.".into());
        }
        let header = record.header.trim().to_lowercase();
        if header.is_empty() || !header.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
            return Err("The header must be a plain header name, like authorization.".into());
        }
        if RESERVED_HEADERS.contains(&header.as_str()) {
            return Err(format!("The {header} header can't carry a key."));
        }
        Ok(Profile {
            name: name.into(),
            key_var: record.key_var.clone(),
            base_url_var: record.base_url_var.clone(),
            upstream: upstream.into(),
            base_path: record.base_path.clone(),
            header,
            bearer: record.bearer,
        })
    }
}

/// The built-in profiles plus every valid custom one in `records`. Invalid
/// custom profiles are skipped, never used half-checked.
pub fn profiles(records: &[dotsafe_core::Record]) -> Vec<Profile> {
    let mut all = builtin_profiles();
    for custom in dotsafe_core::integration::proxy_profiles(records) {
        if let Ok(profile) = Profile::custom(&custom)
            && !all.iter().any(|p| p.key_var == profile.key_var)
        {
            all.push(profile);
        }
    }
    all
}

/// Which of `names` would go through the proxy.
pub fn proxied_names<'a>(names: impl IntoIterator<Item = &'a str>, profiles: &[Profile]) -> Vec<String> {
    let names: Vec<&str> = names.into_iter().collect();
    profiles.iter().filter(|p| names.contains(&p.key_var.as_str())).map(|p| p.key_var.clone()).collect()
}

struct Route {
    upstream: String,
    header: String,
    value: Zeroizing<String>,
}

type Routes = Arc<Mutex<HashMap<String, Route>>>;

/// The proxy server. Start one and keep it for the process's lifetime.
pub struct ProxyServer {
    url: String,
    routes: Routes,
    allow_plain_http: bool,
}

/// Keeps a route alive; dropping it removes the route.
pub struct RouteGuard {
    token: String,
    routes: Routes,
    pub url: String,
}

impl Drop for RouteGuard {
    fn drop(&mut self) {
        if let Ok(mut routes) = self.routes.lock() {
            routes.remove(&self.token);
        }
    }
}

impl ProxyServer {
    pub fn start() -> std::io::Result<ProxyServer> {
        Self::start_inner(false)
    }

    /// For tests: allows `http://127.0.0.1` upstreams.
    #[doc(hidden)]
    pub fn start_allowing_plain_http() -> std::io::Result<ProxyServer> {
        Self::start_inner(true)
    }

    fn start_inner(allow_plain_http: bool) -> std::io::Result<ProxyServer> {
        let server = Server::http("127.0.0.1:0").map_err(std::io::Error::other)?;
        let port = server.server_addr().to_ip().map(|a| a.port()).ok_or_else(|| std::io::Error::other("no port"))?;
        let routes: Routes = Arc::default();
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .max_redirects(0)
            .max_redirects_will_error(false)
            .timeout_connect(Some(Duration::from_secs(15)))
            .build()
            .into();
        let shared = routes.clone();
        std::thread::spawn(move || {
            for request in server.incoming_requests() {
                let (routes, agent) = (shared.clone(), agent.clone());
                std::thread::spawn(move || forward(request, &routes, &agent));
            }
        });
        Ok(ProxyServer { url: format!("http://127.0.0.1:{port}"), routes, allow_plain_http })
    }

    /// Adds a route to `upstream` that sends `value` in `header`.
    pub fn route(&self, upstream: &str, header: &str, value: Zeroizing<String>) -> Result<RouteGuard, String> {
        let upstream = upstream.trim_end_matches('/').to_string();
        let plain_local = self.allow_plain_http && upstream.starts_with("http://127.0.0.1");
        if !upstream.starts_with("https://") && !plain_local {
            return Err(format!("The proxy only forwards to https:// upstreams, not {upstream}."));
        }
        let token: String = uuid::Uuid::new_v4().simple().to_string() + &uuid::Uuid::new_v4().simple().to_string();
        let mut routes = self.routes.lock().map_err(|_| "proxy routes are poisoned".to_string())?;
        routes.insert(token.clone(), Route { upstream, header: header.to_lowercase(), value });
        Ok(RouteGuard { url: format!("{}/{token}", self.url), token, routes: self.routes.clone() })
    }
}

/// The variables a command gets, with known API keys replaced by the proxy.
pub struct Proxied {
    pub env: Vec<(String, String)>,
    /// Variables that go through the proxy.
    pub names: Vec<String>,
    /// Routes for this run; dropped when it ends.
    pub routes: Vec<RouteGuard>,
}

/// Replaces each known key in `vars` with a placeholder and points its SDK
/// at a proxy route holding the real key. A base URL already set in `vars`
/// (for example an Azure endpoint) becomes the route's upstream.
pub fn apply(server: &ProxyServer, vars: Vec<(String, String)>, profiles: &[Profile]) -> Proxied {
    let mut env = vars;
    let mut names = Vec::new();
    let mut routes = Vec::new();
    for profile in profiles {
        let Some(key) = env.iter().find(|(k, _)| *k == profile.key_var).map(|(_, v)| v.clone()) else { continue };
        let custom =
            env.iter().find(|(k, v)| *k == profile.base_url_var && v.starts_with("https://")).map(|(_, v)| v.clone());
        let (upstream, base_path) = match custom {
            Some(url) => (url, ""),
            None => (profile.upstream.clone(), profile.base_path.as_str()),
        };
        let value = Zeroizing::new(if profile.bearer { format!("Bearer {key}") } else { key });
        let Ok(route) = server.route(&upstream, &profile.header, value) else { continue };
        env.retain(|(k, _)| *k != profile.key_var && *k != profile.base_url_var);
        env.push((profile.key_var.clone(), PLACEHOLDER.to_string()));
        env.push((profile.base_url_var.clone(), format!("{}{base_path}", route.url)));
        names.push(profile.key_var.clone());
        routes.push(route);
    }
    Proxied { env, names, routes }
}

/// Request headers never forwarded: auth (replaced by the route's), hop by
/// hop, and encodings (we ask upstream for plain bodies so we can stream them).
const DROP_REQUEST: &[&str] = &[
    "host",
    "authorization",
    "x-api-key",
    "proxy-authorization",
    "proxy-connection",
    "connection",
    "keep-alive",
    "transfer-encoding",
    "te",
    "trailer",
    "upgrade",
    "content-length",
    "accept-encoding",
];
const DROP_RESPONSE: &[&str] =
    &["connection", "keep-alive", "transfer-encoding", "content-length", "trailer", "upgrade"];

fn forward(mut request: tiny_http::Request, routes: &Routes, agent: &ureq::Agent) {
    let path = request.url().to_string();
    let (token, rest) = path.trim_start_matches('/').split_once('/').unwrap_or((path.trim_start_matches('/'), ""));
    let route =
        routes.lock().ok().and_then(|r| r.get(token).map(|r| (r.upstream.clone(), r.header.clone(), r.value.clone())));
    let Some((upstream, header, value)) = route else {
        let _ = request.respond(error(404, "Unknown or expired dotsafe proxy route."));
        return;
    };

    let mut builder =
        ureq::http::Request::builder().method(request.method().as_str()).uri(format!("{upstream}/{rest}"));
    for h in request.headers() {
        let name = h.field.as_str().as_str().to_lowercase();
        if !DROP_REQUEST.contains(&name.as_str()) {
            builder = builder.header(name, h.value.as_str());
        }
    }
    builder = builder.header(header.as_str(), value.as_str()).header("accept-encoding", "identity");

    let mut body = Vec::new();
    if request.as_reader().take(MAX_REQUEST_BODY + 1).read_to_end(&mut body).is_err()
        || body.len() as u64 > MAX_REQUEST_BODY
    {
        let _ = request.respond(error(413, "Request too large for the dotsafe proxy."));
        return;
    }
    let sent = if body.is_empty() {
        builder.body(()).map_err(|e| e.to_string()).and_then(|r| agent.run(r).map_err(|e| e.to_string()))
    } else {
        builder.body(body).map_err(|e| e.to_string()).and_then(|r| agent.run(r).map_err(|e| e.to_string()))
    };
    let upstream_response = match sent {
        Ok(response) => response,
        Err(_) => {
            let _ = request.respond(error(502, "The dotsafe proxy couldn't reach the provider."));
            return;
        }
    };

    let status = upstream_response.status().as_u16();
    let headers: Vec<Header> = upstream_response
        .headers()
        .iter()
        .filter(|(name, _)| !DROP_RESPONSE.contains(&name.as_str()))
        .filter_map(|(name, value)| Header::from_bytes(name.as_str().as_bytes(), value.as_bytes()).ok())
        .collect();
    let reader = upstream_response.into_body().into_reader();
    // No length: tiny_http streams it chunked, so server-sent events flow as they arrive.
    let _ = request.respond(Response::new(StatusCode(status), headers, reader, None, None));
}

fn error(status: u16, message: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    let body = serde_json::json!({ "error": { "message": message, "type": "dotsafe_proxy" } }).to_string();
    Response::from_string(body)
        .with_status_code(status)
        .with_header(Header::from_bytes("content-type", "application/json").expect("valid header"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    /// Method, path, headers and body of a request the stand-in received.
    type Recorded = (String, String, Vec<(String, String)>, String);

    /// A stand-in provider that records the requests it gets.
    fn upstream() -> (String, Arc<StdMutex<Vec<Recorded>>>) {
        let server = Server::http("127.0.0.1:0").unwrap();
        let url = format!("http://127.0.0.1:{}", server.server_addr().to_ip().unwrap().port());
        let seen = Arc::new(StdMutex::new(Vec::new()));
        let log = seen.clone();
        std::thread::spawn(move || {
            for mut req in server.incoming_requests() {
                let headers = req
                    .headers()
                    .iter()
                    .map(|h| (h.field.as_str().as_str().to_lowercase(), h.value.to_string()))
                    .collect();
                let mut body = String::new();
                req.as_reader().read_to_string(&mut body).unwrap();
                log.lock().unwrap().push((req.method().to_string(), req.url().to_string(), headers, body));
                let reply = Response::from_string("data: one\n\ndata: two\n\n")
                    .with_status_code(200)
                    .with_header(Header::from_bytes("content-type", "text/event-stream").unwrap());
                let _ = req.respond(reply);
            }
        });
        (url, seen)
    }

    fn post(url: &str, headers: &[(&str, &str)], body: Option<&str>) -> (u16, String) {
        let agent: ureq::Agent = ureq::Agent::config_builder().http_status_as_error(false).build().into();
        let mut req = agent.post(url);
        for (k, v) in headers {
            req = req.header(*k, *v);
        }
        let mut res = req.send(body.unwrap_or("").as_bytes()).unwrap();
        (res.status().as_u16(), res.body_mut().read_to_string().unwrap())
    }

    #[test]
    fn the_real_key_replaces_the_commands_auth() {
        let (up, seen) = upstream();
        let proxy = ProxyServer::start_allowing_plain_http().unwrap();
        let route = proxy.route(&up, "authorization", Zeroizing::new("Bearer sk-real".into())).unwrap();

        let (status, body) = post(
            &format!("{}/v1/chat/completions?stream=true", route.url),
            &[("authorization", "Bearer dotsafe-proxied"), ("x-api-key", "leak"), ("content-type", "application/json")],
            Some(r#"{"model":"x"}"#),
        );
        assert_eq!(status, 200);
        assert_eq!(body, "data: one\n\ndata: two\n\n", "the response streams back as is");

        let seen = seen.lock().unwrap();
        let (method, path, headers, body) = &seen[0];
        assert_eq!(
            (method.as_str(), path.as_str(), body.as_str()),
            ("POST", "/v1/chat/completions?stream=true", r#"{"model":"x"}"#)
        );
        let header =
            |name: &str| headers.iter().filter(|(k, _)| k == name).map(|(_, v)| v.as_str()).collect::<Vec<_>>();
        assert_eq!(header("authorization"), ["Bearer sk-real"]);
        assert!(header("x-api-key").is_empty(), "auth the command sends never reaches the provider");
        assert_eq!(header("content-type"), ["application/json"]);
    }

    #[test]
    fn routes_end_with_their_guard_and_unknown_tokens_are_refused() {
        let (up, _) = upstream();
        let proxy = ProxyServer::start_allowing_plain_http().unwrap();
        let route = proxy.route(&up, "x-api-key", Zeroizing::new("k".into())).unwrap();
        let url = route.url.clone();
        drop(route);
        assert_eq!(post(&format!("{url}/v1/messages"), &[], None).0, 404);
        assert_eq!(post(&format!("{}/not-a-token/v1", proxy.url), &[], None).0, 404);
    }

    #[test]
    fn only_https_upstreams() {
        let proxy = ProxyServer::start().unwrap();
        assert!(proxy.route("http://api.openai.com", "authorization", Zeroizing::new("k".into())).is_err());
        assert!(proxy.route("http://127.0.0.1:9", "authorization", Zeroizing::new("k".into())).is_err());
        assert!(proxy.route("https://api.openai.com", "authorization", Zeroizing::new("k".into())).is_ok());
    }

    #[test]
    fn known_keys_are_swapped_for_proxy_urls() {
        let proxy = ProxyServer::start().unwrap();
        let vars = vec![
            ("OPENAI_API_KEY".to_string(), "sk-real".to_string()),
            ("ANTHROPIC_API_KEY".to_string(), "ant-real".to_string()),
            ("ANTHROPIC_BASE_URL".to_string(), "https://gateway.example.com/anthropic".to_string()),
            ("DATABASE_URL".to_string(), "postgres://x".to_string()),
        ];
        let p = apply(&proxy, vars, &builtin_profiles());
        let get = |k: &str| p.env.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone()).unwrap();
        assert_eq!(get("OPENAI_API_KEY"), PLACEHOLDER);
        assert_eq!(get("ANTHROPIC_API_KEY"), PLACEHOLDER);
        assert!(get("OPENAI_BASE_URL").starts_with(&proxy.url) && get("OPENAI_BASE_URL").ends_with("/v1"));
        assert!(get("ANTHROPIC_BASE_URL").starts_with(&proxy.url));
        assert_eq!(get("DATABASE_URL"), "postgres://x", "other variables pass through");
        assert_eq!(p.names, ["OPENAI_API_KEY", "ANTHROPIC_API_KEY"]);
        assert!(!p.env.iter().any(|(_, v)| v.contains("real")), "no real key in the command's environment");
        assert_eq!(proxied_names(["OPENAI_API_KEY", "PORT"], &builtin_profiles()), ["OPENAI_API_KEY"]);
    }

    fn custom(upstream: &str, key_var: &str, header: &str) -> dotsafe_core::ProxyProfile {
        dotsafe_core::ProxyProfile {
            id: uuid::Uuid::new_v4(),
            name: "Mistral".into(),
            key_var: key_var.into(),
            base_url_var: "MISTRAL_BASE_URL".into(),
            upstream: upstream.into(),
            base_path: "/v1".into(),
            header: header.into(),
            bearer: true,
            deleted: false,
            updated: dotsafe_core::Stamp::new(1, uuid::Uuid::nil()),
        }
    }

    #[test]
    fn custom_profiles_are_checked_before_use() {
        let ok = Profile::custom(&custom("https://api.mistral.ai/", "MISTRAL_API_KEY", "Authorization")).unwrap();
        assert_eq!((ok.upstream.as_str(), ok.header.as_str()), ("https://api.mistral.ai", "authorization"));
        for (upstream, key, header, why) in [
            ("http://api.mistral.ai", "MISTRAL_API_KEY", "authorization", "https"),
            ("https://user:pw@evil.example", "MISTRAL_API_KEY", "authorization", "plain"),
            ("https://api.mistral.ai?x=1", "MISTRAL_API_KEY", "authorization", "plain"),
            ("https://", "MISTRAL_API_KEY", "authorization", "https://"),
            ("https://api.mistral.ai", "OPENAI_API_KEY", "authorization", "already goes through"),
            ("https://api.mistral.ai", "bad name", "authorization", "valid variable"),
            ("https://api.mistral.ai", "MISTRAL_API_KEY", "x key", "plain header"),
            ("https://api.mistral.ai", "MISTRAL_API_KEY", "host", "can't carry"),
        ] {
            let err = Profile::custom(&custom(upstream, key, header)).unwrap_err();
            assert!(err.contains(why), "{upstream} {key} {header}: {err}");
        }
    }

    #[test]
    fn custom_profiles_route_their_key_like_the_built_in_ones() {
        let proxy = ProxyServer::start().unwrap();
        let records = vec![dotsafe_core::Record::ProxyProfile(custom("https://api.mistral.ai", "MISTRAL_API_KEY", "authorization"))];
        let all = profiles(&records);
        assert_eq!(all.len(), 3);
        let p = apply(&proxy, vec![("MISTRAL_API_KEY".into(), "m-real".into())], &all);
        let get = |k: &str| p.env.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone()).unwrap();
        assert_eq!(get("MISTRAL_API_KEY"), PLACEHOLDER);
        assert!(get("MISTRAL_BASE_URL").starts_with(&proxy.url) && get("MISTRAL_BASE_URL").ends_with("/v1"));
        // An invalid profile in the vault is skipped, not used.
        let bad = vec![dotsafe_core::Record::ProxyProfile(custom("http://evil.example", "MISTRAL_API_KEY", "authorization"))];
        assert_eq!(profiles(&bad).len(), 2);
    }
}
