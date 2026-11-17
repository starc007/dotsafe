//! The share bucket: an S3-compatible bucket, public-read with listing off,
//! holding sealed share snapshots (see `dotsafe_core::share`).
//!
//! The user's devices write it with a storage key from the keychain. A
//! machine reads its snapshot with no key at all, over plain HTTPS, through
//! [`Fetcher`]. Both sides send nothing anywhere else and follow no
//! redirects.

use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use dotsafe_core::share::{ExpiryRule, MAX_SNAPSHOT_BYTES, ShareBucket};
use dotsafe_core::{Error, Result, ShareBucketLocation, crypto};
use rusty_s3::actions::S3Action;
use rusty_s3::{Bucket, Credentials, Method, UrlStyle};
use url::Url;

use crate::S3Credentials;

const SIGNATURE_TTL: Duration = Duration::from_secs(60);
const TIMEOUT: Duration = Duration::from_secs(20);
const DAY_MS: u64 = 24 * 60 * 60 * 1000;
/// Lifecycle rules dotsafe owns start with this; any others are the user's
/// and are kept as they are.
const RULE_PREFIX: &str = "dotsafe-share-";
const S3_NS: &str = "http://s3.amazonaws.com/doc/2006-03-01/";

/// Writes to a share bucket over the S3 API.
pub struct S3ShareBucket {
    bucket: Bucket,
    credentials: Credentials,
    public_url: String,
    location: String,
    agent: ureq::Agent,
}

impl S3ShareBucket {
    pub fn new(location: &ShareBucketLocation, credentials: &S3Credentials) -> Result<S3ShareBucket> {
        let endpoint = Url::parse(location.endpoint.trim())
            .map_err(|_| Error::Storage(format!("{:?} isn't a valid endpoint URL", location.endpoint)))?;
        let style = match endpoint.host_str() {
            Some(host) if host.ends_with("amazonaws.com") => UrlStyle::VirtualHost,
            _ => UrlStyle::Path,
        };
        let bucket = Bucket::new(endpoint, style, location.bucket.clone(), location.region.clone())
            .map_err(|e| Error::Storage(format!("invalid bucket settings: {e}")))?;
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(TIMEOUT))
            .max_redirects(0)
            .build()
            .into();
        Ok(S3ShareBucket {
            bucket,
            credentials: Credentials::new(&credentials.access_key_id, credentials.secret_access_key.as_str()),
            public_url: dotsafe_core::share::public_base(&location.public_url)?,
            location: location.describe(),
            agent,
        })
    }

    fn unreachable(&self) -> Error {
        Error::Unreachable(self.location.clone())
    }

    fn failure(&self, status: u16, body: &str) -> Error {
        match status {
            401 | 403 => Error::Storage(format!(
                "{} refused the access key. Check the key and that it can write objects and lifecycle rules in this bucket",
                self.location
            )),
            404 if body.contains("NoSuchBucket") => Error::Storage(format!("bucket {:?} doesn't exist", self.bucket.name())),
            301..=308 => Error::Storage(format!("{} redirected the request; check the endpoint and region", self.location)),
            501 => Error::Storage(format!("{} doesn't support lifecycle rules, so it can't hold shares", self.location)),
            408 | 429 | 500.. => self.unreachable(),
            status => Error::Storage(format!("{} answered {status}: {}", self.location, s3_message(body))),
        }
    }

    /// A signed URL for the bucket's `?lifecycle` subresource.
    fn lifecycle_url(&self, method: Method) -> Url {
        rusty_s3::signing::sign(
            &jiff::Timestamp::now(),
            method,
            self.bucket.base_url().clone(),
            self.credentials.key(),
            self.credentials.secret(),
            None,
            self.bucket.region(),
            SIGNATURE_TTL.as_secs(),
            std::iter::once(("lifecycle", "")),
            std::iter::empty(),
        )
    }

    /// The bucket's lifecycle rules, as XML `<Rule>` elements. None set is
    /// an empty list.
    fn rules(&self) -> Result<Vec<String>> {
        let url = self.lifecycle_url(Method::Get);
        let mut res = self.agent.get(url.as_str()).call().map_err(|_| self.unreachable())?;
        let status = res.status().as_u16();
        let body = res.body_mut().read_to_string().map_err(|_| self.unreachable())?;
        match status {
            200 => Ok(rule_elements(&body)),
            404 if body.contains("NoSuchLifecycleConfiguration") => Ok(Vec::new()),
            status => Err(self.failure(status, &body)),
        }
    }
}

impl ShareBucket for S3ShareBucket {
    fn location(&self) -> String {
        self.location.clone()
    }

    fn public_url(&self) -> String {
        self.public_url.clone()
    }

    fn put(&self, key: &str, bytes: &[u8]) -> Result<()> {
        let url = self.bucket.put_object(Some(&self.credentials), key).sign(SIGNATURE_TTL);
        let mut res = self
            .agent
            .put(url.as_str())
            .header("content-type", "application/json")
            // A CDN in front of the bucket must never serve a stale or
            // revoked snapshot.
            .header("cache-control", "no-store")
            .send(bytes)
            .map_err(|_| self.unreachable())?;
        match res.status().as_u16() {
            200 => Ok(()),
            status => {
                let body = res.body_mut().read_to_string().unwrap_or_default();
                Err(self.failure(status, &body))
            }
        }
    }

    fn delete(&self, key: &str) -> Result<()> {
        let url = self.bucket.delete_object(Some(&self.credentials), key).sign(SIGNATURE_TTL);
        let mut res = self.agent.delete(url.as_str()).call().map_err(|_| self.unreachable())?;
        match res.status().as_u16() {
            200 | 204 | 404 => Ok(()),
            status => {
                let body = res.body_mut().read_to_string().unwrap_or_default();
                Err(self.failure(status, &body))
            }
        }
    }

    fn presign_put(&self, key: &str, valid_secs: u64) -> Result<String> {
        Ok(self.bucket.put_object(Some(&self.credentials), key).sign(Duration::from_secs(valid_secs)).to_string())
    }

    fn presign_delete(&self, key: &str, valid_secs: u64) -> Result<String> {
        Ok(self.bucket.delete_object(Some(&self.credentials), key).sign(Duration::from_secs(valid_secs)).to_string())
    }

    fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let url = self.bucket.get_object(Some(&self.credentials), key).sign(SIGNATURE_TTL);
        let mut res = self.agent.get(url.as_str()).call().map_err(|_| self.unreachable())?;
        match res.status().as_u16() {
            200 => {
                let limit = dotsafe_core::share::MAX_MESSAGE_BYTES.max(MAX_SNAPSHOT_BYTES) as u64;
                Ok(Some(res.body_mut().with_config().limit(limit).read_to_vec().map_err(|_| self.unreachable())?))
            }
            404 => Ok(None),
            status => {
                let body = res.body_mut().read_to_string().unwrap_or_default();
                Err(self.failure(status, &body))
            }
        }
    }

    fn etag(&self, key: &str) -> Result<Option<String>> {
        let url = self.bucket.head_object(Some(&self.credentials), key).sign(SIGNATURE_TTL);
        let res = self.agent.head(url.as_str()).call().map_err(|_| self.unreachable())?;
        match res.status().as_u16() {
            200 => Ok(Some(res.headers().get("etag").and_then(|v| v.to_str().ok()).unwrap_or_default().to_string())),
            404 => Ok(None),
            status => Err(self.failure(status, "")),
        }
    }

    fn set_expiry_rules(&self, rules: &[ExpiryRule]) -> Result<()> {
        let today = now_ms();
        let mut elements: Vec<String> =
            self.rules()?.into_iter().filter(|r| !rule_id(r).is_some_and(|id| id.starts_with(RULE_PREFIX))).collect();
        elements.extend(rules.iter().map(|r| rule_xml(r, today)));
        // A configuration needs at least one rule; with none, remove it.
        if elements.is_empty() {
            let url = self.lifecycle_url(Method::Delete);
            let mut res = self.agent.delete(url.as_str()).call().map_err(|_| self.unreachable())?;
            return match res.status().as_u16() {
                200 | 204 | 404 => Ok(()),
                status => {
                    let body = res.body_mut().read_to_string().unwrap_or_default();
                    Err(self.failure(status, &body))
                }
            };
        }
        let body = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><LifecycleConfiguration xmlns=\"{S3_NS}\">{}</LifecycleConfiguration>",
            elements.concat()
        );
        let checksum = STANDARD.encode(hex_bytes(&crypto::sha256_hex(body.as_bytes())));
        let url = self.lifecycle_url(Method::Put);
        let mut res = self
            .agent
            .put(url.as_str())
            .header("content-type", "application/xml")
            // S3 requires a checksum of the body for this call.
            .header("x-amz-sdk-checksum-algorithm", "SHA256")
            .header("x-amz-checksum-sha256", checksum.as_str())
            .send(body.as_bytes())
            .map_err(|_| self.unreachable())?;
        match res.status().as_u16() {
            200 | 204 => Ok(()),
            status => {
                let body = res.body_mut().read_to_string().unwrap_or_default();
                Err(self.failure(status, &body))
            }
        }
    }
}

/// Fetches public snapshots: HTTPS only, no redirects, no credentials, no
/// caches.
pub struct Fetcher {
    agent: ureq::Agent,
    https_only: bool,
}

impl Default for Fetcher {
    fn default() -> Self {
        Fetcher::new()
    }
}

impl Fetcher {
    pub fn new() -> Fetcher {
        Fetcher { agent: fetch_agent(true), https_only: true }
    }

    /// Also fetches plain `http://`. For tests against a local fake; it
    /// can't be turned on from config or the environment.
    pub fn allowing_http_for_tests() -> Fetcher {
        Fetcher { agent: fetch_agent(false), https_only: false }
    }

    /// The object at `url`, or `None` if there isn't one: the share was
    /// revoked or expired, or nothing was pushed yet.
    pub fn get(&self, url: &str) -> Result<Option<Vec<u8>>> {
        let parsed = Url::parse(url).map_err(|_| Error::Invalid(format!("{url:?} isn't a URL")))?;
        let host = parsed.host_str().unwrap_or_default().to_string();
        if self.https_only && parsed.scheme() != "https" {
            return Err(Error::Invalid("share snapshots are only fetched over HTTPS".into()));
        }
        let mut res = self
            .agent
            .get(parsed.as_str())
            .header("cache-control", "no-cache")
            .call()
            .map_err(|_| Error::Unreachable(host.clone()))?;
        match res.status().as_u16() {
            200 => {
                let bytes = res
                    .body_mut()
                    .with_config()
                    .limit(MAX_SNAPSHOT_BYTES as u64)
                    .read_to_vec()
                    .map_err(|_| Error::Unreachable(host))?;
                Ok(Some(bytes))
            }
            // A public bucket without listing answers 403 for missing objects.
            403 | 404 => Ok(None),
            301..=308 => Err(Error::Storage(format!("{host} redirected the request; dotsafe doesn't follow redirects"))),
            408 | 429 | 500.. => Err(Error::Unreachable(host)),
            status => Err(Error::Storage(format!("{host} answered {status}"))),
        }
    }

    /// Approve mode, on the machine: PUTs `bytes` to a presigned URL from
    /// its mailbox. HTTPS only, no redirects.
    pub fn put_signed(&self, url: &str, bytes: &[u8]) -> Result<()> {
        let (parsed, host) = self.check(url)?;
        let res = self
            .agent
            .put(parsed.as_str())
            .header("content-type", "application/json")
            .header("cache-control", "no-store")
            .send(bytes)
            .map_err(|_| Error::Unreachable(host.clone()))?;
        signed_outcome(res.status().as_u16(), &host)
    }

    /// Approve mode, on the machine: DELETEs through a presigned URL.
    pub fn delete_signed(&self, url: &str) -> Result<()> {
        let (parsed, host) = self.check(url)?;
        let res = self.agent.delete(parsed.as_str()).call().map_err(|_| Error::Unreachable(host.clone()))?;
        signed_outcome(res.status().as_u16(), &host)
    }

    fn check(&self, url: &str) -> Result<(Url, String)> {
        let parsed = Url::parse(url).map_err(|_| Error::Invalid("the share's mailbox has a malformed URL".into()))?;
        if self.https_only && parsed.scheme() != "https" {
            return Err(Error::Invalid("the share's mailbox URLs must be HTTPS".into()));
        }
        let host = parsed.host_str().unwrap_or_default().to_string();
        Ok((parsed, host))
    }
}

fn signed_outcome(status: u16, host: &str) -> Result<()> {
    match status {
        200 | 204 | 404 => Ok(()),
        401 | 403 => Err(Error::Storage(format!(
            "{host} refused the share's upload URL; it may have expired, so unlock dotsafe on one of your devices to renew it"
        ))),
        301..=308 => Err(Error::Storage(format!("{host} redirected the request; dotsafe doesn't follow redirects"))),
        408 | 429 | 500.. => Err(Error::Unreachable(host.to_string())),
        status => Err(Error::Storage(format!("{host} answered {status}"))),
    }
}

fn fetch_agent(https_only: bool) -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(TIMEOUT))
        .max_redirects(0)
        .https_only(https_only)
        .build()
        .into()
}

/// Checks a share bucket works before it's used: an object written through
/// the API can be read at the public URL, isn't cached, and the bucket
/// doesn't list its contents. Returns warnings that don't stop it working.
pub fn probe(bucket: &S3ShareBucket, fetcher: &Fetcher) -> Result<Vec<String>> {
    // Shaped like a snapshot's key, so the same bucket policy covers it.
    let key = format!("s/{}", uuid::Uuid::new_v4().simple());
    let marker = format!("dotsafe share bucket check {}", now_ms());
    bucket.put(&key, marker.as_bytes())?;
    let public = format!("{}/{key}", bucket.public_url);
    let read = fetcher.get(&public);
    let _ = bucket.delete(&key);
    match read? {
        Some(bytes) if bytes == marker.as_bytes() => {}
        Some(_) => return Err(Error::Storage(format!("{public} served something other than what was written"))),
        None => {
            return Err(Error::Storage(format!(
                "{public} wasn't readable. Make the bucket publicly readable at {} (an R2 custom domain, or a bucket policy allowing GetObject on s/*)",
                bucket.public_url
            )));
        }
    }
    let mut warnings = Vec::new();
    for listing in [format!("{}/", bucket.public_url), format!("{}/?list-type=2", bucket.public_url)] {
        if let Ok(Some(body)) = fetcher.get(&listing)
            && String::from_utf8_lossy(&body).contains("<ListBucketResult")
        {
            return Err(Error::Storage(format!(
                "{listing} lists the bucket's contents. Turn listing off, so share paths can't be found"
            )));
        }
    }
    bucket.rules()?;
    if bucket.public_url.ends_with(".r2.dev") || bucket.public_url.contains(".r2.dev/") {
        warnings.push("r2.dev addresses are rate-limited and not meant for production; use a custom domain".into());
    }
    Ok(warnings)
}

/// The `<Rule>…</Rule>` elements of a lifecycle configuration, verbatim.
fn rule_elements(xml: &str) -> Vec<String> {
    let mut rules = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find("<Rule>") {
        let Some(len) = rest[start..].find("</Rule>") else { break };
        let end = start + len + "</Rule>".len();
        rules.push(rest[start..end].to_string());
        rest = &rest[end..];
    }
    rules
}

fn rule_id(rule: &str) -> Option<&str> {
    let start = rule.find("<ID>")? + "<ID>".len();
    let len = rule[start..].find("</ID>")?;
    Some(&rule[start..start + len])
}

/// One share's rule: delete everything under its prefix at the first
/// midnight UTC after it expires (S3 only takes midnight dates), and never
/// a date already past.
fn rule_xml(rule: &ExpiryRule, now: u64) -> String {
    let day = rule.expires.div_ceil(DAY_MS).max(now / DAY_MS + 1);
    let date = jiff::Timestamp::from_second((day * 86_400) as i64)
        .map(|t| t.strftime("%Y-%m-%dT00:00:00.000Z").to_string())
        .unwrap_or_else(|_| "2100-01-01T00:00:00.000Z".into());
    format!(
        "<Rule><ID>{RULE_PREFIX}{}</ID><Filter><Prefix>{}</Prefix></Filter><Status>Enabled</Status><Expiration><Date>{date}</Date></Expiration></Rule>",
        rule.share, rule.prefix
    )
}

fn s3_message(body: &str) -> String {
    let start = body.find("<Message>").map(|i| i + "<Message>".len());
    match start.and_then(|s| body[s..].find("</Message>").map(|len| &body[s..s + len])) {
        Some(message) => message.to_string(),
        None => body.chars().take(200).collect(),
    }
}

fn hex_bytes(hex: &str) -> Vec<u8> {
    (0..hex.len()).step_by(2).filter_map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok()).collect()
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_expire_at_the_next_midnight_and_never_in_the_past() {
        let share = uuid::Uuid::nil();
        let rule = |expires| ExpiryRule { share, prefix: "s/ab".into(), expires };
        // 2026-11-17 13:00 UTC → 2026-11-18.
        let xml = rule_xml(&rule(1_794_920_400_000), 0);
        assert!(xml.contains("<Date>2026-11-18T00:00:00.000Z</Date>"), "{xml}");
        assert!(xml.contains("<Prefix>s/ab</Prefix>") && xml.contains("<ID>dotsafe-share-00000000-"));
        // Exactly midnight stays that midnight.
        assert!(rule_xml(&rule(1_794_960_000_000), 0).contains("2026-11-18T00"));
        // Already expired: tomorrow, not a date S3 might refuse.
        assert!(rule_xml(&rule(1), 1_794_920_400_000).contains("2026-11-18T00"));
    }

    #[test]
    fn users_own_rules_are_kept() {
        let xml = "<LifecycleConfiguration><Rule><ID>logs</ID><Status>Enabled</Status></Rule><Rule><ID>dotsafe-share-1</ID></Rule></LifecycleConfiguration>";
        let rules = rule_elements(xml);
        assert_eq!(rules.len(), 2);
        assert_eq!(rule_id(&rules[0]), Some("logs"));
        assert_eq!(rule_id(&rules[1]), Some("dotsafe-share-1"));
    }
}
