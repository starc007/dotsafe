//! A vault in an S3-compatible bucket (AWS S3, Cloudflare R2, Backblaze B2,
//! MinIO). Requests are signed with SigV4 presigned URLs, and writes use
//! `If-None-Match: *` / `If-Match: <etag>`, which both S3 and R2 support, so
//! two devices can't overwrite each other.

use std::time::Duration;

use dotsafe_core::store::{Cond, ETag, Object, ObjectMeta, Put, Store};
use dotsafe_core::{Error, Result, S3Location};
use rusty_s3::actions::{ListObjectsV2, S3Action};
use rusty_s3::{Bucket, Credentials, UrlStyle};
use url::Url;
use zeroize::Zeroizing;

/// How long a signed URL stays valid. Requests are sent right away.
const SIGNATURE_TTL: Duration = Duration::from_secs(60);
const TIMEOUT: Duration = Duration::from_secs(20);

/// An access key for the bucket. Keep it out of logs.
#[derive(Clone)]
pub struct S3Credentials {
    pub access_key_id: String,
    pub secret_access_key: Zeroizing<String>,
}

impl std::fmt::Debug for S3Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("S3Credentials").field("access_key_id", &self.access_key_id).finish_non_exhaustive()
    }
}

pub struct S3Store {
    bucket: Bucket,
    credentials: Credentials,
    prefix: String,
    location: String,
    agent: ureq::Agent,
}

impl S3Store {
    pub fn new(location: &S3Location, credentials: &S3Credentials) -> Result<S3Store> {
        let endpoint = Url::parse(location.endpoint.trim())
            .map_err(|_| Error::Storage(format!("{:?} isn't a valid endpoint URL", location.endpoint)))?;
        // AWS prefers bucket-in-hostname URLs; R2, B2 and MinIO all accept paths.
        let style = match endpoint.host_str() {
            Some(host) if host.ends_with("amazonaws.com") => UrlStyle::VirtualHost,
            _ => UrlStyle::Path,
        };
        let bucket = Bucket::new(endpoint, style, location.bucket.clone(), location.region.clone())
            .map_err(|e| Error::Storage(format!("invalid bucket settings: {e}")))?;
        let agent =
            ureq::Agent::config_builder().http_status_as_error(false).timeout_global(Some(TIMEOUT)).build().into();
        Ok(S3Store {
            bucket,
            credentials: Credentials::new(&credentials.access_key_id, credentials.secret_access_key.as_str()),
            prefix: location.prefix.trim_matches('/').to_owned(),
            location: location.describe(),
            agent,
        })
    }

    fn object(&self, key: &str) -> String {
        if self.prefix.is_empty() { key.to_owned() } else { format!("{}/{key}", self.prefix) }
    }

    fn unreachable(&self) -> Error {
        Error::Unreachable(self.location.clone())
    }

    /// Maps statuses that mean the same thing for every request.
    fn failure(&self, status: u16, body: &str) -> Error {
        match status {
            401 | 403 => Error::Storage(format!(
                "{} refused the access key. Check the key and that it can read and write this bucket",
                self.location
            )),
            404 if body.contains("NoSuchBucket") => {
                Error::Storage(format!("bucket {:?} doesn't exist", self.bucket.name()))
            }
            408 | 429 | 500.. => self.unreachable(),
            status => Error::Storage(format!("{} answered {status}", self.location)),
        }
    }
}

fn etag_of(res: &ureq::http::Response<ureq::Body>) -> Option<ETag> {
    res.headers().get("etag").and_then(|v| v.to_str().ok()).map(str::to_owned)
}

impl Store for S3Store {
    fn location(&self) -> String {
        self.location.clone()
    }

    fn list(&self, dir: &str) -> Result<Vec<String>> {
        Ok(self.list_meta(dir)?.into_iter().map(|m| m.key).collect())
    }

    fn list_meta(&self, dir: &str) -> Result<Vec<ObjectMeta>> {
        let full_dir = match (self.prefix.as_str(), dir) {
            ("", "") => String::new(),
            ("", dir) => format!("{dir}/"),
            (prefix, "") => format!("{prefix}/"),
            (prefix, dir) => format!("{prefix}/{dir}/"),
        };
        let strip = if self.prefix.is_empty() { String::new() } else { format!("{}/", self.prefix) };
        let mut metas = Vec::new();
        let mut token: Option<String> = None;
        loop {
            let mut action = self.bucket.list_objects_v2(Some(&self.credentials));
            action.with_prefix(full_dir.as_str());
            action.with_delimiter("/");
            if let Some(token) = &token {
                action.with_continuation_token(token.as_str());
            }
            let url = action.sign(SIGNATURE_TTL);
            let mut res = self.agent.get(url.as_str()).call().map_err(|_| self.unreachable())?;
            let status = res.status().as_u16();
            let body = res.body_mut().read_to_string().map_err(|_| self.unreachable())?;
            if status != 200 {
                return Err(self.failure(status, &body));
            }
            let page = ListObjectsV2::parse_response(&body)
                .map_err(|e| Error::Storage(format!("couldn't read the bucket listing: {e}")))?;
            for item in page.contents {
                let Some(key) = item.key.strip_prefix(&strip) else { continue };
                let name = key.rsplit('/').next().unwrap_or(key);
                if !name.starts_with('.') && !name.is_empty() {
                    metas.push(ObjectMeta { key: key.to_owned(), etag: item.etag });
                }
            }
            match page.next_continuation_token {
                Some(next) => token = Some(next),
                None => break,
            }
        }
        metas.sort_by(|a, b| a.key.cmp(&b.key));
        Ok(metas)
    }

    fn get(&self, key: &str) -> Result<Option<Object>> {
        let object = self.object(key);
        let url = self.bucket.get_object(Some(&self.credentials), &object).sign(SIGNATURE_TTL);
        let mut res = self.agent.get(url.as_str()).call().map_err(|_| self.unreachable())?;
        match res.status().as_u16() {
            200 => {
                let etag = etag_of(&res).ok_or_else(|| Error::Storage("the bucket sent no ETag".into()))?;
                let bytes = res.body_mut().read_to_vec().map_err(|_| self.unreachable())?;
                Ok(Some(Object { bytes, etag }))
            }
            404 => {
                let body = res.body_mut().read_to_string().unwrap_or_default();
                if body.contains("NoSuchBucket") { Err(self.failure(404, &body)) } else { Ok(None) }
            }
            status => {
                let body = res.body_mut().read_to_string().unwrap_or_default();
                Err(self.failure(status, &body))
            }
        }
    }

    fn put(&self, key: &str, bytes: &[u8], cond: Cond) -> Result<Put> {
        let object = self.object(key);
        let url = self.bucket.put_object(Some(&self.credentials), &object).sign(SIGNATURE_TTL);
        let mut req = self.agent.put(url.as_str()).header("content-type", "application/json");
        req = match &cond {
            Cond::Any => req,
            Cond::IfAbsent => req.header("if-none-match", "*"),
            Cond::IfMatch(etag) => req.header("if-match", etag.as_str()),
        };
        let mut res = req.send(bytes).map_err(|_| self.unreachable())?;
        match res.status().as_u16() {
            200 => {
                let etag = etag_of(&res).ok_or_else(|| Error::Storage("the bucket sent no ETag".into()))?;
                Ok(Put::Written(etag))
            }
            // 412: the condition failed. 409: S3 saw a concurrent conditional
            // write to the same key. Either way, someone else got there first.
            412 | 409 => Ok(Put::Conflict),
            status => {
                let body = res.body_mut().read_to_string().unwrap_or_default();
                Err(self.failure(status, &body))
            }
        }
    }

    fn delete(&self, key: &str) -> Result<()> {
        let object = self.object(key);
        let url = self.bucket.delete_object(Some(&self.credentials), &object).sign(SIGNATURE_TTL);
        let mut res = self.agent.delete(url.as_str()).call().map_err(|_| self.unreachable())?;
        match res.status().as_u16() {
            200 | 204 | 404 => Ok(()),
            status => {
                let body = res.body_mut().read_to_string().unwrap_or_default();
                Err(self.failure(status, &body))
            }
        }
    }
}
