//! The HTTP calls for each service.

use std::collections::BTreeMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use dotsafe_core::{Connection, ProviderLink, Target};
use serde_json::{Value, json};
use url::Url;
use zeroize::Zeroizing;

use crate::{Error, Result, service, sigv4};

const TIMEOUT: Duration = Duration::from_secs(30);
/// New AWS access keys take a few seconds to work everywhere.
const AWS_PROPAGATION: Duration = Duration::from_secs(3);
const AWS_ATTEMPTS: u32 = 6;
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// A key a provider just issued.
pub struct NewKey {
    pub value: Zeroizing<String>,
    /// The provider's id for it, needed to revoke it later.
    pub key_id: String,
}

pub struct Client {
    agent: ureq::Agent,
    /// Replaces every service's API address. Only for tests, which point it
    /// at a local fake; it can't be set from config or the environment.
    base: Option<Url>,
    /// How long to wait between checks of a new AWS key.
    aws_wait: Duration,
}

impl Default for Client {
    fn default() -> Self {
        Client::new()
    }
}

impl Client {
    pub fn new() -> Client {
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(TIMEOUT))
            // A redirect could carry the token elsewhere.
            .max_redirects(0)
            .https_only(true)
            .build()
            .into();
        Client { agent, base: None, aws_wait: AWS_PROPAGATION }
    }

    /// A client that sends every request to `base` over plain HTTP. For
    /// tests against a local fake server.
    pub fn with_base_for_tests(base: &str) -> Client {
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(TIMEOUT))
            .max_redirects(0)
            .build()
            .into();
        Client { agent, base: Some(Url::parse(base).expect("test base URL")), aws_wait: Duration::ZERO }
    }

    /// Asks the provider for a new key named `name`.
    pub fn create_key(&self, connection: &Connection, token: &str, link: &ProviderLink, name: &str) -> Result<NewKey> {
        match connection.service.as_str() {
            "openai" => {
                let project = setting(&connection.settings, "project_id")?;
                let url = self.url("OpenAI", OPENAI, &["organization", "projects", project, "service_accounts"])?;
                let body = self.send("OpenAI", "POST", url, &bearer(token), Some(json!({ "name": name })))?;
                let key_id = body["id"].as_str();
                let value = body["api_key"]["value"].as_str();
                match (key_id, value) {
                    (Some(id), Some(value)) => Ok(NewKey { value: Zeroizing::new(value.into()), key_id: id.into() }),
                    _ => Err(Error::Invalid("OpenAI's answer had no new key in it".into())),
                }
            }
            "cloudflare_token" => {
                let token_id = setting(&link.settings, "token_id")?;
                let url = self.url("Cloudflare", CLOUDFLARE, &cloudflare_tokens(connection, &[token_id, "value"]))?;
                let body = self.send("Cloudflare", "PUT", url, &bearer(token), Some(json!({})))?;
                match body["result"].as_str() {
                    Some(value) => Ok(NewKey { value: Zeroizing::new(value.into()), key_id: token_id.into() }),
                    None => Err(Error::Invalid("Cloudflare's answer had no new token in it".into())),
                }
            }
            "aws_iam" => {
                let user = setting(&link.settings, "user_name")?;
                let body = sigv4::form(&[("Action", "CreateAccessKey"), ("UserName", user), ("Version", "2010-05-08")]);
                let xml = self.aws("iam", IAM, &connection_key_id(connection)?, token, &body)?;
                match (xml_tag(&xml, "AccessKeyId"), xml_tag(&xml, "SecretAccessKey")) {
                    (Some(id), Some(secret)) => Ok(NewKey { value: Zeroizing::new(secret), key_id: id }),
                    _ => Err(Error::Invalid("AWS's answer had no new access key in it".into())),
                }
            }
            other => match service(other) {
                Ok(s) if s.guided => Err(Error::Invalid(format!("{} keys are made in its console; paste the new one", s.label))),
                _ => Err(not_a("provider", other)),
            },
        }
    }

    /// Checks a new key works, by calling the provider with it. AWS keys
    /// also need their access key ID.
    pub fn verify_key(&self, connection: &Connection, value: &str, key_id: Option<&str>) -> Result<()> {
        match connection.service.as_str() {
            "openai" => {
                let url = self.url("OpenAI", OPENAI, &["models"])?;
                self.send("OpenAI", "GET", url, &bearer(value), None).map(|_| ())
            }
            "cloudflare_token" => {
                let url = self.url("Cloudflare", CLOUDFLARE, &cloudflare_tokens(connection, &["verify"]))?;
                let body = self.send("Cloudflare", "GET", url, &bearer(value), None)?;
                match body["result"]["status"].as_str() {
                    Some("active") => Ok(()),
                    status => Err(Error::Invalid(format!("Cloudflare says the new token is {}", status.unwrap_or("unusable")))),
                }
            }
            "anthropic" => {
                let url = self.url("Anthropic", ANTHROPIC, &["models"])?;
                self.send("Anthropic", "GET", url, &anthropic(value), None).map(|_| ())
            }
            "stripe" => {
                let url = self.url("Stripe", STRIPE, &["balance"])?;
                self.send("Stripe", "GET", url, &bearer(value), None).map(|_| ())
            }
            "aws_iam" => {
                let key_id = key_id.ok_or_else(|| Error::Invalid("the new access key ID is missing".into()))?;
                let body = sigv4::form(&[("Action", "GetCallerIdentity"), ("Version", "2011-06-15")]);
                let mut last = None;
                for attempt in 0..AWS_ATTEMPTS {
                    if attempt > 0 {
                        std::thread::sleep(self.aws_wait);
                    }
                    match self.aws("sts", STS, key_id, value, &body) {
                        Ok(_) => return Ok(()),
                        // Not known everywhere yet: try again shortly.
                        Err(e @ Error::Refused { status: 403, .. }) => last = Some(e),
                        Err(e) => return Err(e),
                    }
                }
                Err(last.expect("at least one attempt"))
            }
            other => Err(not_a("provider", other)),
        }
    }

    /// The provider's id for the key `value`, where it can be found from the
    /// key itself. Anthropic lists keys with a hint (its start and end).
    pub fn find_key_id(&self, connection: &Connection, token: &str, value: &str) -> Result<Option<String>> {
        if connection.service != "anthropic" {
            return Ok(None);
        }
        let mut after: Option<String> = None;
        let mut found = Vec::new();
        loop {
            let mut url = self.url("Anthropic", ANTHROPIC, &["organizations", "api_keys"])?;
            url.query_pairs_mut().append_pair("limit", "1000");
            if let Some(after) = &after {
                url.query_pairs_mut().append_pair("after_id", after);
            }
            let page = self.send("Anthropic", "GET", url, &anthropic(token), None)?;
            for key in page["data"].as_array().into_iter().flatten() {
                let hint = key["partial_key_hint"].as_str().unwrap_or_default();
                if key["status"] == "active"
                    && let (Some(id), Some((start, end))) = (key["id"].as_str(), hint.split_once("..."))
                    && !start.is_empty()
                    && value.starts_with(start)
                    && value.ends_with(end)
                {
                    found.push(id.to_string());
                }
            }
            match (page["has_more"].as_bool(), page["last_id"].as_str()) {
                (Some(true), Some(last)) => after = Some(last.to_string()),
                _ => break,
            }
        }
        // Two keys with the same hint can't be told apart.
        Ok(if found.len() == 1 { found.pop() } else { None })
    }

    /// Revokes a key the provider issued. A key that's already gone counts
    /// as revoked.
    pub fn revoke_key(&self, connection: &Connection, token: &str, link: &ProviderLink, key_id: &str) -> Result<()> {
        match connection.service.as_str() {
            "openai" => {
                let project = setting(&connection.settings, "project_id")?;
                let url = self.url("OpenAI", OPENAI, &["organization", "projects", project, "service_accounts", key_id])?;
                gone_is_fine(self.send("OpenAI", "DELETE", url, &bearer(token), None))
            }
            // Rolling already ended the old token.
            "cloudflare_token" => Ok(()),
            "anthropic" => {
                let url = self.url("Anthropic", ANTHROPIC, &["organizations", "api_keys", key_id])?;
                gone_is_fine(self.send("Anthropic", "POST", url, &anthropic(token), Some(json!({ "status": "inactive" }))))
            }
            "aws_iam" => {
                let user = setting(&link.settings, "user_name")?;
                let body = sigv4::form(&[
                    ("Action", "DeleteAccessKey"),
                    ("AccessKeyId", key_id),
                    ("UserName", user),
                    ("Version", "2010-05-08"),
                ]);
                match self.aws("iam", IAM, &connection_key_id(connection)?, token, &body) {
                    Err(Error::Refused { status: 404, .. }) => Ok(()),
                    other => other.map(|_| ()),
                }
            }
            other => Err(not_a("provider that revokes through an API", other)),
        }
    }

    /// Sets `value` as the target's variable, creating or replacing it.
    pub fn push(&self, connection: &Connection, token: &str, target: &Target, value: &str) -> Result<()> {
        match connection.service.as_str() {
            "vercel" => {
                let project = setting(&target.settings, "project")?;
                let mut url = self.url("Vercel", VERCEL, &["v10", "projects", project, "env"])?;
                url.query_pairs_mut().append_pair("upsert", "true");
                if let Some(team) = connection.settings.get("team_id").filter(|t| !t.is_empty()) {
                    url.query_pairs_mut().append_pair("teamId", team);
                }
                let environments: Vec<&str> = target
                    .settings
                    .get("environments")
                    .map(|e| e.split(',').map(str::trim).filter(|e| !e.is_empty()).collect())
                    .filter(|e: &Vec<&str>| !e.is_empty())
                    .unwrap_or_else(|| vec!["production", "preview", "development"]);
                let body = json!({ "key": target.name, "value": value, "type": "encrypted", "target": environments });
                self.send("Vercel", "POST", url, &bearer(token), Some(body)).map(|_| ())
            }
            "cloudflare_workers" => {
                let account = setting(&connection.settings, "account_id")?;
                let script = setting(&target.settings, "script")?;
                let url = self.url("Cloudflare", CLOUDFLARE, &["accounts", account, "workers", "scripts", script, "secrets"])?;
                let body = json!({ "name": target.name, "text": value, "type": "secret_text" });
                self.send("Cloudflare", "PUT", url, &bearer(token), Some(body)).map(|_| ())
            }
            "netlify" => {
                let account = setting(&connection.settings, "account_id")?;
                let site = setting(&target.settings, "site_id")?;
                let contexts = netlify_contexts(target)?;
                // Setting each context's value keeps the variable's other
                // contexts, scopes and secret flag as they are.
                for (i, context) in contexts.iter().enumerate() {
                    let mut url = self.url("Netlify", NETLIFY, &["accounts", account, "env", &target.name])?;
                    url.query_pairs_mut().append_pair("site_id", site);
                    let body = json!({ "context": context, "value": value });
                    match self.send("Netlify", "PATCH", url, &bearer(token), Some(body)) {
                        Ok(_) => {}
                        // Not there yet: create it with every context at once.
                        Err(Error::Refused { status: 404, .. }) if i == 0 => {
                            let mut url = self.url("Netlify", NETLIFY, &["accounts", account, "env"])?;
                            url.query_pairs_mut().append_pair("site_id", site);
                            let values: Vec<Value> = contexts.iter().map(|c| json!({ "context": c, "value": value })).collect();
                            // Netlify only keeps values secret in explicit
                            // contexts, and shows `dev` values anyway.
                            let secret = !contexts.iter().any(|c| *c == "all" || *c == "dev");
                            let mut var = json!({ "key": target.name, "values": values, "is_secret": secret });
                            if secret {
                                var["scopes"] = json!(["builds", "functions", "runtime"]);
                            }
                            return self.send("Netlify", "POST", url, &bearer(token), Some(json!([var]))).map(|_| ());
                        }
                        Err(e) => return Err(e),
                    }
                }
                Ok(())
            }
            "fly" => {
                let app = setting(&target.settings, "app")?;
                let url = self.url("Fly.io", FLY, &["v1", "apps", app, "secrets", &target.name])?;
                self.send("Fly.io", "POST", url, &fly(token), Some(json!({ "value": value }))).map(|_| ())
            }
            "aws_ssm" => {
                let region = aws_region(connection)?;
                let mut body = json!({ "Name": ssm_name(target)?, "Value": value, "Type": "SecureString", "Overwrite": true });
                if let Some(key) = target.settings.get("kms_key_id").filter(|k| !k.trim().is_empty()) {
                    body["KeyId"] = json!(key.trim());
                }
                let body = Zeroizing::new(body.to_string());
                let base = format!("https://ssm.{region}.amazonaws.com/");
                let call = AwsCall {
                    service: "ssm",
                    base: &base,
                    region,
                    content_type: sigv4::JSON_CONTENT_TYPE,
                    target: Some("AmazonSSM.PutParameter"),
                };
                self.aws_call(&call, &connection_key_id(connection)?, token, &body).map(|_| ())
            }
            other => Err(not_a("deploy target", other)),
        }
    }

    /// `base` with each segment appended, percent-encoded so settings can't
    /// change the path.
    fn url(&self, service: &'static str, base: &str, segments: &[&str]) -> Result<Url> {
        let mut url = match &self.base {
            Some(base) => base.clone(),
            None => Url::parse(base).expect("built-in API addresses parse"),
        };
        url.path_segments_mut()
            .map_err(|_| Error::Invalid(format!("bad address for {service}")))?
            .pop_if_empty()
            .extend(segments);
        Ok(url)
    }

    fn send(
        &self,
        service: &'static str,
        method: &str,
        url: Url,
        headers: &[(&'static str, Zeroizing<String>)],
        body: Option<Value>,
    ) -> Result<Value> {
        let uri = url.as_str();
        macro_rules! with_headers {
            ($request:expr) => {{
                let mut request = $request;
                for (name, value) in headers {
                    request = request.header(*name, value.as_str());
                }
                request
            }};
        }
        let response = match method {
            "GET" => with_headers!(self.agent.get(uri)).call(),
            "DELETE" => with_headers!(self.agent.delete(uri)).call(),
            "POST" => with_headers!(self.agent.post(uri)).send_json(body.unwrap_or_default()),
            "PUT" => with_headers!(self.agent.put(uri)).send_json(body.unwrap_or_default()),
            "PATCH" => with_headers!(self.agent.patch(uri)).send_json(body.unwrap_or_default()),
            _ => unreachable!("only the methods above are used"),
        };
        let mut response = response.map_err(|_| Error::Unreachable(service))?;
        let status = response.status().as_u16();
        let text = response.body_mut().read_to_string().unwrap_or_default();
        let body: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        if (200..300).contains(&status) {
            return Ok(body);
        }
        Err(Error::Refused { service, status, message: error_message(&body).unwrap_or_else(|| reason(status)) })
    }

    /// A signed POST to an AWS query API (IAM, STS). Returns the XML body.
    fn aws(&self, aws_service: &str, base: &str, access_key_id: &str, secret: &str, body: &str) -> Result<String> {
        let call = AwsCall { service: aws_service, base, region: "us-east-1", content_type: sigv4::CONTENT_TYPE, target: None };
        self.aws_call(&call, access_key_id, secret, body)
    }

    /// A signed POST to an AWS API. Query APIs answer in XML, JSON APIs
    /// (SSM) in JSON; errors from either are turned into `Refused`.
    fn aws_call(&self, call: &AwsCall, access_key_id: &str, secret: &str, body: &str) -> Result<String> {
        let url = self.url("AWS", call.base, &[])?;
        let host = match (url.host_str(), url.port()) {
            (Some(host), Some(port)) => format!("{host}:{port}"),
            (Some(host), None) => host.to_string(),
            _ => return Err(Error::Invalid("bad address for AWS".into())),
        };
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let amz_date = sigv4::amz_date(now);
        let request = sigv4::Request {
            host: &host,
            region: call.region,
            service: call.service,
            content_type: call.content_type,
            target: call.target,
            body,
            amz_date: &amz_date,
        };
        let authorization = Zeroizing::new(sigv4::authorization(&request, access_key_id, secret));
        let mut request = self
            .agent
            .post(url.as_str())
            .header("Content-Type", call.content_type)
            .header("X-Amz-Date", &amz_date)
            .header("Authorization", authorization.as_str());
        if let Some(target) = call.target {
            request = request.header("X-Amz-Target", target);
        }
        let mut response = request.send(body.as_bytes()).map_err(|_| Error::Unreachable("AWS"))?;
        let status = response.status().as_u16();
        let text = response.body_mut().read_to_string().unwrap_or_default();
        if (200..300).contains(&status) {
            return Ok(text);
        }
        let json: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        // JSON errors name the type as `com.amazonaws…#AccessDeniedException`.
        let code = xml_tag(&text, "Code")
            .or_else(|| json["__type"].as_str().map(|t| t.rsplit('#').next().unwrap_or(t).to_string()));
        let detail = xml_tag(&text, "Message").or_else(|| {
            json["message"].as_str().or_else(|| json["Message"].as_str()).map(str::to_string)
        });
        let message = match (&code, detail) {
            (Some(code), Some(message)) => format!("{code}: {message}"),
            (Some(code), None) => code.clone(),
            (None, Some(message)) => message,
            _ => reason(status),
        };
        let status = if code.as_deref() == Some("NoSuchEntity") { 404 } else { status };
        Err(Error::Refused { service: "AWS", status, message })
    }
}

/// Where and how to send a signed AWS request.
struct AwsCall<'a> {
    service: &'a str,
    base: &'a str,
    region: &'a str,
    content_type: &'a str,
    target: Option<&'a str>,
}

const OPENAI: &str = "https://api.openai.com/v1";
const CLOUDFLARE: &str = "https://api.cloudflare.com/client/v4";
const VERCEL: &str = "https://api.vercel.com";
const ANTHROPIC: &str = "https://api.anthropic.com/v1";
const STRIPE: &str = "https://api.stripe.com/v1";
const IAM: &str = "https://iam.amazonaws.com/";
const STS: &str = "https://sts.amazonaws.com/";
const NETLIFY: &str = "https://api.netlify.com/api/v1";
const FLY: &str = "https://api.machines.dev";

/// Netlify deploy contexts dotsafe can set (`branch` would need a branch name).
const NETLIFY_CONTEXTS: &[&str] = &["all", "dev", "dev-server", "branch-deploy", "deploy-preview", "production"];

fn bearer(token: &str) -> Vec<(&'static str, Zeroizing<String>)> {
    vec![("Authorization", Zeroizing::new(format!("Bearer {token}")))]
}

/// Fly.io tokens from `fly tokens create` start with `FlyV1 ` and are sent
/// as they are; older ones are bearer tokens.
fn fly(token: &str) -> Vec<(&'static str, Zeroizing<String>)> {
    let token = token.trim();
    if token.starts_with("FlyV1 ") {
        vec![("Authorization", Zeroizing::new(token.to_string()))]
    } else {
        bearer(token)
    }
}

/// The target's Netlify contexts, checked against the ones Netlify takes.
fn netlify_contexts(target: &Target) -> Result<Vec<&str>> {
    let contexts: Vec<&str> = match target.settings.get("contexts") {
        Some(list) => list.split(',').map(str::trim).filter(|c| !c.is_empty()).collect(),
        None => Vec::new(),
    };
    if contexts.is_empty() {
        return Ok(vec!["production", "deploy-preview", "branch-deploy"]);
    }
    match contexts.iter().find(|c| !NETLIFY_CONTEXTS.contains(c)) {
        Some(bad) => Err(Error::Invalid(format!("Netlify has no {bad:?} deploy context; use {}", NETLIFY_CONTEXTS.join(", ")))),
        None => Ok(contexts),
    }
}

/// The connection's AWS region. It becomes part of the SSM host name, so
/// only region-shaped names (`us-east-1`, `us-gov-west-1`) are accepted.
fn aws_region(connection: &Connection) -> Result<&str> {
    let region = setting(&connection.settings, "region")?.trim();
    let parts: Vec<&str> = region.split('-').collect();
    let shaped = parts.len() >= 3
        && parts[..parts.len() - 1].iter().all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_lowercase()))
        && parts[parts.len() - 1].bytes().all(|b| b.is_ascii_digit())
        && !parts[parts.len() - 1].is_empty();
    if shaped { Ok(region) } else { Err(Error::Invalid(format!("{region:?} isn't an AWS region, e.g. us-east-1"))) }
}

/// The SSM parameter name: `<path>/<name>`, or the name alone.
fn ssm_name(target: &Target) -> Result<String> {
    let path = target.settings.get("path").map(|p| p.trim().trim_end_matches('/')).unwrap_or_default();
    if path.is_empty() {
        return Ok(target.name.clone());
    }
    let allowed = |b: u8| b.is_ascii_alphanumeric() || b"_.-/".contains(&b);
    if !path.starts_with('/') || !path.bytes().all(allowed) || path.contains("//") {
        return Err(Error::Invalid(format!(
            "{path:?} isn't a parameter path; use letters, numbers, _ . - and /, starting with /"
        )));
    }
    Ok(format!("{path}/{}", target.name))
}

fn anthropic(key: &str) -> Vec<(&'static str, Zeroizing<String>)> {
    vec![("x-api-key", Zeroizing::new(key.to_string())), ("anthropic-version", Zeroizing::new(ANTHROPIC_VERSION.into()))]
}

fn gone_is_fine(result: Result<Value>) -> Result<()> {
    match result {
        Err(Error::Refused { status: 404, .. }) => Ok(()),
        other => other.map(|_| ()),
    }
}

fn connection_key_id(connection: &Connection) -> Result<String> {
    setting(&connection.settings, "access_key_id").map(str::to_string)
}

/// The text of the first `<tag>…</tag>` in an AWS XML answer.
fn xml_tag(xml: &str, tag: &str) -> Option<String> {
    let start = xml.find(&format!("<{tag}>"))? + tag.len() + 2;
    let end = start + xml[start..].find(&format!("</{tag}>"))?;
    Some(xml[start..end].trim().to_string())
}

/// Path to user-owned tokens, or account-owned ones when the connection has
/// an account id.
fn cloudflare_tokens<'a>(connection: &'a Connection, rest: &[&'a str]) -> Vec<&'a str> {
    let mut path = match connection.settings.get("account_id").filter(|a| !a.is_empty()) {
        Some(account) => vec!["accounts", account.as_str(), "tokens"],
        None => vec!["user", "tokens"],
    };
    path.extend_from_slice(rest);
    path
}

fn setting<'a>(settings: &'a BTreeMap<String, String>, key: &str) -> Result<&'a str> {
    settings.get(key).map(String::as_str).filter(|v| !v.trim().is_empty()).ok_or_else(|| Error::Invalid(format!("{key} isn't set")))
}

fn not_a(what: &str, service: &str) -> Error {
    Error::Invalid(format!("{service} isn't a {what}"))
}

/// The message in OpenAI, Anthropic, Stripe and Vercel (`error.message`) or
/// Cloudflare (`errors[0].message`) error bodies.
fn error_message(body: &Value) -> Option<String> {
    body["error"]["message"]
        .as_str()
        .or_else(|| body["errors"][0]["message"].as_str())
        .or_else(|| body["error"].as_str())
        .map(str::to_string)
}

fn reason(status: u16) -> String {
    match status {
        401 => "the token was rejected".into(),
        403 => "the token isn't allowed to do this".into(),
        404 => "not found; check the IDs in the settings".into(),
        429 => "too many requests; try again in a minute".into(),
        _ => format!("HTTP {status}"),
    }
}
