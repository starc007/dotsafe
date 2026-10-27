//! A vault in a folder on a WebDAV server: Nextcloud, ownCloud, many NAS
//! boxes, Fastmail and others. Like a bucket, it only ever holds encrypted
//! objects, and it's used behind [`dotsafe_core::CachedStore`] so dotsafe
//! works offline.
//!
//! Writes are conditional with `If-Match` / `If-None-Match: *` (RFC 7232),
//! which the common servers honour, so two devices can't silently overwrite
//! each other.

use std::time::Duration;

use base64::Engine;
use dotsafe_core::store::{Cond, ETag, Object, Put, Store};
use dotsafe_core::{Error, Result, WebDavLocation};
use url::Url;
use zeroize::Zeroizing;

const TIMEOUT: Duration = Duration::from_secs(30);
const PROPFIND: &str = r#"<?xml version="1.0" encoding="utf-8"?><d:propfind xmlns:d="DAV:"><d:prop><d:getetag/><d:resourcetype/></d:prop></d:propfind>"#;

pub struct WebDavStore {
    base: Url,
    /// `Basic <base64(user:password)>`.
    auth: Zeroizing<String>,
    location: String,
    agent: ureq::Agent,
}

impl WebDavStore {
    pub fn new(location: &WebDavLocation, password: &str) -> Result<WebDavStore> {
        let mut url = location.url.trim().to_string();
        if !url.ends_with('/') {
            url.push('/');
        }
        let base = Url::parse(&url).map_err(|_| Error::Storage(format!("{:?} isn't a valid URL", location.url)))?;
        let local = matches!(base.host_str(), Some("localhost" | "127.0.0.1"));
        if base.scheme() != "https" && !(base.scheme() == "http" && local) {
            return Err(Error::Storage("a WebDAV address must start with https://".into()));
        }
        let credentials = Zeroizing::new(format!("{}:{password}", location.username));
        let auth = Zeroizing::new(format!("Basic {}", base64::engine::general_purpose::STANDARD.encode(credentials.as_bytes())));
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(TIMEOUT))
            // A redirect could send the password elsewhere.
            .max_redirects(0)
            .allow_non_standard_methods(true)
            .build()
            .into();
        Ok(WebDavStore { base, auth, location: location.describe(), agent })
    }

    fn url(&self, key: &str) -> Result<Url> {
        let mut url = self.base.clone();
        {
            let mut segments = url.path_segments_mut().map_err(|_| Error::Storage("bad WebDAV address".into()))?;
            segments.pop_if_empty();
            for part in key.split('/').filter(|p| !p.is_empty()) {
                segments.push(part);
            }
        }
        Ok(url)
    }

    fn unreachable(&self) -> Error {
        Error::Unreachable(self.location.clone())
    }

    fn failure(&self, status: u16) -> Error {
        match status {
            401 | 403 => Error::Storage(format!("{} refused the username or password", self.location)),
            408 | 429 | 500.. => self.unreachable(),
            status => Error::Storage(format!("{} answered HTTP {status}", self.location)),
        }
    }

    /// Sends a request with any method (WebDAV adds PROPFIND and MKCOL).
    fn send(
        &self,
        method: &str,
        url: &Url,
        headers: &[(&str, &str)],
        body: &[u8],
    ) -> Result<(u16, Option<String>, Vec<u8>)> {
        let mut builder = ureq::http::Request::builder()
            .method(method)
            .uri(url.as_str())
            .header("Authorization", self.auth.as_str());
        for (name, value) in headers {
            builder = builder.header(*name, *value);
        }
        let request = builder.body(body.to_vec()).map_err(|e| Error::Storage(e.to_string()))?;
        let mut response = self.agent.run(request).map_err(|_| self.unreachable())?;
        let status = response.status().as_u16();
        let etag = response.headers().get("etag").and_then(|v| v.to_str().ok()).map(str::to_string);
        let bytes = response.body_mut().read_to_vec().map_err(|_| self.unreachable())?;
        Ok((status, etag, bytes))
    }

    /// Creates the collection `dir` (one level), when a PUT finds it missing.
    fn mkcol(&self, dir: &str) -> Result<()> {
        let mut url = self.url(dir)?;
        url.set_path(&format!("{}/", url.path().trim_end_matches('/')));
        match self.send("MKCOL", &url, &[], &[])?.0 {
            200..=299 | 405 => Ok(()), // 405: it exists already.
            status => Err(self.failure(status)),
        }
    }
}

impl Store for WebDavStore {
    fn location(&self) -> String {
        self.location.clone()
    }

    fn list(&self, dir: &str) -> Result<Vec<String>> {
        let mut url = self.url(dir)?;
        url.set_path(&format!("{}/", url.path().trim_end_matches('/')));
        let own_path = url.path().to_string();
        let (status, _, body) = self.send("PROPFIND", &url, &[("Depth", "1"), ("Content-Type", "application/xml")], PROPFIND.as_bytes())?;
        match status {
            207 => {}
            404 => return Ok(Vec::new()),
            status => return Err(self.failure(status)),
        }
        let xml = String::from_utf8_lossy(&body);
        let mut keys: Vec<String> = hrefs(&xml)
            .into_iter()
            .filter_map(|href| {
                // An href is a path, or a full URL on some servers.
                let path = Url::parse(&href).map(|u| u.path().to_string()).unwrap_or(href);
                let path = percent_decode(&path);
                if path.ends_with('/') || path.trim_end_matches('/') == percent_decode(&own_path).trim_end_matches('/') {
                    return None; // the folder itself, or a subfolder
                }
                let name = path.rsplit('/').next()?.to_string();
                (!name.is_empty() && !name.starts_with('.'))
                    .then(|| if dir.is_empty() { name.clone() } else { format!("{}/{name}", dir.trim_matches('/')) })
            })
            .collect();
        keys.sort();
        keys.dedup();
        Ok(keys)
    }

    fn get(&self, key: &str) -> Result<Option<Object>> {
        let (status, etag, bytes) = self.send("GET", &self.url(key)?, &[], &[])?;
        match status {
            200 => Ok(Some(Object { etag: etag.unwrap_or_else(|| dotsafe_core::store::etag(&bytes)), bytes })),
            404 => Ok(None),
            status => Err(self.failure(status)),
        }
    }

    fn put(&self, key: &str, bytes: &[u8], cond: Cond) -> Result<Put> {
        let url = self.url(key)?;
        let header: Vec<(&str, String)> = match &cond {
            Cond::Any => Vec::new(),
            Cond::IfAbsent => vec![("If-None-Match", "*".to_string())],
            Cond::IfMatch(etag) => vec![("If-Match", etag.clone())],
        };
        let headers: Vec<(&str, &str)> = header.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let mut attempt = self.send("PUT", &url, &headers, bytes)?;
        // The folder (`records/`) doesn't exist yet: make it and try again.
        if attempt.0 == 409
            && let Some((dir, _)) = key.rsplit_once('/')
        {
            self.mkcol(dir)?;
            attempt = self.send("PUT", &url, &headers, bytes)?;
        }
        match attempt {
            (200..=299, etag, _) => {
                let etag: ETag = match etag {
                    Some(etag) => etag,
                    // Some servers don't return one; read it back.
                    None => self.get(key)?.map(|o| o.etag).unwrap_or_else(|| dotsafe_core::store::etag(bytes)),
                };
                Ok(Put::Written(etag))
            }
            (412, _, _) => Ok(Put::Conflict),
            (status, _, _) => Err(self.failure(status)),
        }
    }

    fn delete(&self, key: &str) -> Result<()> {
        match self.send("DELETE", &self.url(key)?, &[], &[])?.0 {
            200..=299 | 404 => Ok(()),
            status => Err(self.failure(status)),
        }
    }
}

/// Every `<…href>…</…href>` in a multistatus answer, whatever the namespace
/// prefix (`d:`, `D:` or none).
fn hrefs(xml: &str) -> Vec<String> {
    let mut out = Vec::new();
    let lower = xml.to_ascii_lowercase();
    let mut from = 0;
    while let Some(open) = lower[from..].find("href>").map(|i| i + from) {
        let tag_start = lower[..open].rfind('<').unwrap_or(open);
        if lower[tag_start..open].starts_with("</") {
            from = open + 5;
            continue;
        }
        let start = open + 5;
        let Some(end) = lower[start..].find("</").map(|i| i + start) else { break };
        out.push(xml[start..end].trim().replace("&amp;", "&"));
        from = end;
    }
    out
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(byte) = u8::from_str_radix(&s[i + 1..i + 3], 16)
        {
            out.push(byte);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hrefs_with_any_prefix() {
        let xml = r#"<d:multistatus xmlns:d="DAV:"><d:response><d:href>/dav/v/</d:href></d:response>
            <D:response><D:href>/dav/v/vault.json</D:href></D:response><response><href>/dav/v/a%20b.json</href></response></d:multistatus>"#;
        assert_eq!(hrefs(xml), ["/dav/v/", "/dav/v/vault.json", "/dav/v/a%20b.json"]);
        assert_eq!(percent_decode("/dav/v/a%20b.json"), "/dav/v/a b.json");
    }
}
