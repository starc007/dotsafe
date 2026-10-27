//! Talking to the services secrets come from and go to: providers that issue
//! keys (OpenAI service accounts, Cloudflare API tokens, AWS IAM access
//! keys), providers whose keys you create by hand and dotsafe checks and
//! revokes (Anthropic, Stripe), and deploy targets that hold them (Vercel,
//! Cloudflare Workers).
//!
//! Everything runs on the device with the user's own admin tokens, which are
//! secrets in the vault. Calls are HTTPS only and never follow redirects, so
//! a token only ever goes to the service it belongs to. [`rotate`] ties the
//! calls to the vault: create, save, verify, promote, push, revoke.

mod client;
pub mod rotate;
mod sigv4;

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
    /// The value names another shared secret; it's stored as that secret's
    /// id. AWS uses it for the secret that gets the new access key ID.
    pub secret_ref: bool,
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
    /// Providers: dotsafe can't create keys. You make one in the console and
    /// paste it; dotsafe checks it, makes it live and pushes it.
    pub guided: bool,
    /// Providers: revoking happens in the console, not through an API. dotsafe
    /// links there, then marks the old key retired.
    pub manual_revoke: bool,
    /// Whether the connection needs an admin token at all.
    pub needs_token: bool,
    /// Where keys are managed, for guided steps.
    pub console_url: Option<&'static str>,
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
            required: true, secret_ref: false
        }],
        binding_fields: &[],
        overlap: true,
        guided: false,
        manual_revoke: false,
        needs_token: true,
        console_url: Some("https://platform.openai.com/api-keys"),
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
            required: false, secret_ref: false
        }],
        binding_fields: &[Field {
            key: "token_id",
            label: "Token ID",
            hint: "The ID of the token this secret holds, from the API Tokens page.",
            required: true, secret_ref: false
        }],
        overlap: false,
        guided: false,
        manual_revoke: false,
        needs_token: true,
        console_url: Some("https://dash.cloudflare.com/profile/api-tokens"),
    },
    Service {
        id: "anthropic",
        label: "Anthropic",
        kind: Kind::Provider,
        token_help: "An Admin key (sk-ant-admin…) from console.anthropic.com → Settings → Admin keys. dotsafe uses it to switch old keys off.",
        connection_fields: &[],
        binding_fields: &[],
        overlap: true,
        guided: true,
        manual_revoke: false,
        needs_token: true,
        console_url: Some("https://console.anthropic.com/settings/keys"),
    },
    Service {
        id: "stripe",
        label: "Stripe",
        kind: Kind::Provider,
        token_help: "No token needed: Stripe has no API for managing keys. dotsafe checks each new key with itself.",
        connection_fields: &[],
        binding_fields: &[],
        overlap: true,
        guided: true,
        manual_revoke: true,
        needs_token: false,
        console_url: Some("https://dashboard.stripe.com/apikeys"),
    },
    Service {
        id: "aws_iam",
        label: "AWS IAM access key",
        kind: Kind::Provider,
        token_help: "The secret access key of an IAM user allowed to call iam:CreateAccessKey and iam:DeleteAccessKey on the user below.",
        connection_fields: &[Field {
            key: "access_key_id",
            label: "Admin access key ID",
            hint: "AKIA…, of the IAM user dotsafe calls IAM as.",
            required: true,
            secret_ref: false,
        }],
        binding_fields: &[
            Field {
                key: "user_name",
                label: "IAM user",
                hint: "The user whose access key this secret holds.",
                required: true,
                secret_ref: false,
            },
            Field {
                key: "access_key_id_secret",
                label: "Shared secret for the key ID",
                hint: "Optional: the shared secret that holds AWS_ACCESS_KEY_ID. It gets the new ID on every rotation.",
                required: false,
                secret_ref: true,
            },
        ],
        overlap: true,
        guided: false,
        manual_revoke: false,
        needs_token: true,
        console_url: Some("https://console.aws.amazon.com/iam/home#/users"),
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
            required: false, secret_ref: false
        }],
        binding_fields: &[
            Field { key: "project", label: "Project", hint: "The project's name or ID.", required: true, secret_ref: false },
            Field {
                key: "environments",
                label: "Environments",
                hint: "Comma-separated: production, preview, development. Defaults to all three.",
                required: false, secret_ref: false
            },
        ],
        overlap: false,
        guided: false,
        manual_revoke: false,
        needs_token: true,
        console_url: None,
    },
    Service {
        id: "cloudflare_workers",
        label: "Cloudflare Workers",
        kind: Kind::Target,
        token_help: "A Cloudflare API token with the \"Workers Scripts: Edit\" permission.",
        connection_fields: &[Field { key: "account_id", label: "Account ID", hint: "From the dashboard's sidebar.", required: true, secret_ref: false }],
        binding_fields: &[Field { key: "script", label: "Worker", hint: "The Worker's name.", required: true, secret_ref: false }],
        overlap: false,
        guided: false,
        manual_revoke: false,
        needs_token: true,
        console_url: None,
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
