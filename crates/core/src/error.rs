use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("wrong password or recovery key")]
    InvalidCredentials,
    #[error("vault header failed authentication; it may be corrupted or tampered with")]
    HeaderTampered,
    #[error("record {0} failed authentication; it may be corrupted or tampered with")]
    RecordTampered(Uuid),
    #[error("the vault's head failed authentication; it may be corrupted or tampered with")]
    HeadTampered,
    #[error("record {0} kept changing while it was being saved; try again")]
    Busy(Uuid),
    #[error("unsupported {kind} version {version}")]
    UnsupportedVersion { kind: &'static str, version: u32 },
    #[error("invalid vault data: {0}")]
    Format(String),
    #[error("a vault already exists at {0}")]
    AlreadyExists(String),
    #[error("no vault found at {0}")]
    NotFound(String),
    /// The store couldn't be reached (offline, timeout, server error). Worth
    /// retrying later; a cached vault keeps working meanwhile.
    #[error("couldn't reach {0}")]
    Unreachable(String),
    /// The store refused the request (bad credentials, missing bucket, ...).
    #[error("storage error: {0}")]
    Storage(String),
    #[error("system random number generator failed")]
    Rng,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
