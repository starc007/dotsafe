//! The HTTP calls for each service.

use std::collections::BTreeMap;
use std::time::Duration;

use dotsafe_core::{Connection, ProviderLink, Target};
use serde_json::{Value, json};
use url::Url;
use zeroize::Zeroizing;

use crate::{Error, Result};

const TIMEOUT: Duration = Duration::from_secs(30);

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
        Client { agent, base: None }
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
        Client { agent, base: Some(Url::parse(base).expect("test base URL")) }
    }

    /// Asks the provider for a new key named `name`.
    pub fn create_key(&self, connection: &Connection, token: &str, link: &ProviderLink, name: &str) -> Result<NewKey> {
        match connection.service.as_str() {
            "openai" => {
                let project = setting(&connection.settings, "project_id")?;
                let url = self.url("OpenAI", OPENAI, &["organization", "projects", project, "service_accounts"])?;
                let body = self.send("OpenAI", "POST", url, token, Some(json!({ "name": name })))?;
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
                let body = self.send("Cloudflare", "PUT", url, token, Some(json!({})))?;
                match body["result"].as_str() {
                    Some(value) => Ok(NewKey { value: Zeroizing::new(value.into()), key_id: token_id.into() }),
                    None => Err(Error::Invalid("Cloudflare's answer had no new token in it".into())),
                }
            }
            other => Err(not_a("provider", other)),
        }
    }

    /// Checks a new key works, by calling the provider with it.
    pub fn verify_key(&self, connection: &Connection, value: &str) -> Result<()> {
        match connection.service.as_str() {
            "openai" => {
                let url = self.url("OpenAI", OPENAI, &["models"])?;
                self.send("OpenAI", "GET", url, value, None).map(|_| ())
            }
            "cloudflare_token" => {
                let url = self.url("Cloudflare", CLOUDFLARE, &cloudflare_tokens(connection, &["verify"]))?;
                let body = self.send("Cloudflare", "GET", url, value, None)?;
                match body["result"]["status"].as_str() {
                    Some("active") => Ok(()),
                    status => Err(Error::Invalid(format!("Cloudflare says the new token is {}", status.unwrap_or("unusable")))),
                }
            }
            other => Err(not_a("provider", other)),
        }
    }

    /// Revokes a key the provider issued. A key that's already gone counts
    /// as revoked.
    pub fn revoke_key(&self, connection: &Connection, token: &str, key_id: &str) -> Result<()> {
        match connection.service.as_str() {
            "openai" => {
                let project = setting(&connection.settings, "project_id")?;
                let url = self.url("OpenAI", OPENAI, &["organization", "projects", project, "service_accounts", key_id])?;
                match self.send("OpenAI", "DELETE", url, token, None) {
                    Err(Error::Refused { status: 404, .. }) => Ok(()),
                    other => other.map(|_| ()),
                }
            }
            // Rolling already ended the old token.
            "cloudflare_token" => Ok(()),
            other => Err(not_a("provider", other)),
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
                self.send("Vercel", "POST", url, token, Some(body)).map(|_| ())
            }
            "cloudflare_workers" => {
                let account = setting(&connection.settings, "account_id")?;
                let script = setting(&target.settings, "script")?;
                let url = self.url("Cloudflare", CLOUDFLARE, &["accounts", account, "workers", "scripts", script, "secrets"])?;
                let body = json!({ "name": target.name, "text": value, "type": "secret_text" });
                self.send("Cloudflare", "PUT", url, token, Some(body)).map(|_| ())
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

    fn send(&self, service: &'static str, method: &str, url: Url, token: &str, body: Option<Value>) -> Result<Value> {
        let auth = Zeroizing::new(format!("Bearer {token}"));
        let uri = url.as_str();
        let request = match method {
            "GET" => self.agent.get(uri).header("Authorization", auth.as_str()).call(),
            "DELETE" => self.agent.delete(uri).header("Authorization", auth.as_str()).call(),
            "POST" => self.agent.post(uri).header("Authorization", auth.as_str()).send_json(body.unwrap_or_default()),
            "PUT" => self.agent.put(uri).header("Authorization", auth.as_str()).send_json(body.unwrap_or_default()),
            _ => unreachable!("only the methods above are used"),
        };
        let mut response = request.map_err(|_| Error::Unreachable(service))?;
        let status = response.status().as_u16();
        let text = response.body_mut().read_to_string().unwrap_or_default();
        let body: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        if (200..300).contains(&status) {
            return Ok(body);
        }
        Err(Error::Refused { service, status, message: error_message(&body).unwrap_or_else(|| reason(status)) })
    }
}

const OPENAI: &str = "https://api.openai.com/v1";
const CLOUDFLARE: &str = "https://api.cloudflare.com/client/v4";
const VERCEL: &str = "https://api.vercel.com";

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

/// The message in OpenAI and Vercel (`error.message`) or Cloudflare
/// (`errors[0].message`) error bodies.
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
