//! Talking to the services secrets come from and go to: providers that issue
//! keys (OpenAI service accounts, Cloudflare API tokens) and deploy targets
//! that hold them (Vercel, Cloudflare Workers).
//!
//! Everything runs on the device with the user's own admin tokens, which are
//! secrets in the vault. Calls are HTTPS only and never follow redirects, so
//! a token only ever goes to the service it belongs to. [`rotate`] ties the
//! calls to the vault: create, save, verify, promote, push, revoke.

mod client;
pub mod rotate;

pub use client::{Client, NewKey};

/// What went wrong talking to a service.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("couldn't reach {0}; check your connection")]
    Unreachable(&'static str),
    #[error("{service} said no ({status}): {message}")]
    Refused { service: &'static str, status: u16, message: String },
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Vault(#[from] dotsafe_core::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Whether a service issues keys or receives them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Provider,
    Target,
}

/// A setting a service needs, asked for when connecting or binding.
#[derive(Clone, Copy, Debug)]
pub struct Field {
    pub key: &'static str,
    pub label: &'static str,
    pub hint: &'static str,
    pub required: bool,
}

/// One integration and what it needs.
#[derive(Debug)]
pub struct Service {
    /// Stored in [`dotsafe_core::Connection::service`].
    pub id: &'static str,
    pub label: &'static str,
    pub kind: Kind,
    /// What the admin token must be allowed to do, and where to make one.
    pub token_help: &'static str,
    /// Settings for the account, asked for when connecting.
    pub connection_fields: &'static [Field],
    /// Settings for each secret (provider) or target, asked for when binding.
    pub binding_fields: &'static [Field],
    /// Providers: the old key keeps working after a new one is issued, until
    /// it's revoked. Cloudflare's roll ends the old one at once.
    pub overlap: bool,
}

pub const SERVICES: &[Service] = &[
    Service {
        id: "openai",
        label: "OpenAI",
        kind: Kind::Provider,
        token_help: "An admin key from platform.openai.com → Organization → Admin keys.",
        connection_fields: &[Field {
            key: "project_id",
            label: "Project ID",
            hint: "proj_…, from the project's settings. New keys belong to a service account in it.",
            required: true,
        }],
        binding_fields: &[],
        overlap: true,
    },
    Service {
        id: "cloudflare_token",
        label: "Cloudflare API token",
        kind: Kind::Provider,
        token_help: "A Cloudflare API token with the \"API Tokens: Edit\" permission.",
        connection_fields: &[Field {
            key: "account_id",
            label: "Account ID",
            hint: "Only for account-owned tokens. Leave empty for tokens on your user.",
            required: false,
        }],
        binding_fields: &[Field {
            key: "token_id",
            label: "Token ID",
            hint: "The ID of the token this secret holds, from the API Tokens page.",
            required: true,
        }],
        overlap: false,
    },
    Service {
        id: "vercel",
        label: "Vercel",
        kind: Kind::Target,
        token_help: "A Vercel access token from vercel.com/account/tokens, scoped to the team.",
        connection_fields: &[Field {
            key: "team_id",
            label: "Team ID",
            hint: "team_…, for team projects. Leave empty for personal ones.",
            required: false,
        }],
        binding_fields: &[
            Field { key: "project", label: "Project", hint: "The project's name or ID.", required: true },
            Field {
                key: "environments",
                label: "Environments",
                hint: "Comma-separated: production, preview, development. Defaults to all three.",
                required: false,
            },
        ],
        overlap: false,
    },
    Service {
        id: "cloudflare_workers",
        label: "Cloudflare Workers",
        kind: Kind::Target,
        token_help: "A Cloudflare API token with the \"Workers Scripts: Edit\" permission.",
        connection_fields: &[Field { key: "account_id", label: "Account ID", hint: "From the dashboard's sidebar.", required: true }],
        binding_fields: &[Field { key: "script", label: "Worker", hint: "The Worker's name.", required: true }],
        overlap: false,
    },
];

/// The service with this id.
pub fn service(id: &str) -> Result<&'static Service> {
    SERVICES.iter().find(|s| s.id == id).ok_or_else(|| Error::Invalid(format!("dotsafe doesn't know {id:?}")))
}

/// Checks `settings` against `fields`: required ones are present, unknown
/// ones are refused.
pub fn check_settings(fields: &[Field], settings: &std::collections::BTreeMap<String, String>) -> Result<()> {
    for field in fields.iter().filter(|f| f.required) {
        if settings.get(field.key).is_none_or(|v| v.trim().is_empty()) {
            return Err(Error::Invalid(format!("{} is required", field.label)));
        }
    }
    if let Some(key) = settings.keys().find(|k| !fields.iter().any(|f| f.key == k.as_str())) {
        return Err(Error::Invalid(format!("unknown setting {key:?}")));
    }
    Ok(())
}
