//! Shared core for dotsafe: vault format, encryption and sync.
//!
//! Both the desktop app and the CLI depend on this crate so they read and
//! write the same vault on disk. See `docs/vault-format.md` for the format.

pub mod crypto;
pub mod error;
pub mod header;
pub mod model;
pub mod vault;

pub use crypto::KdfParams;
pub use error::{Error, Result};
pub use header::RecoveryKey;
pub use model::{Environment, Project, Record, Stamp};
pub use vault::Vault;

/// Version of the core library, reported by the app and CLI.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
