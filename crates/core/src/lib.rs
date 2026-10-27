//! Shared core for dotsafe: vault format, encryption and sync.
//!
//! Both the desktop app and the CLI depend on this crate so they read and
//! write the same vault on disk. See `docs/vault-format.md` for the format.

pub mod audit;
pub mod cache;
pub mod config;
pub mod crypto;
pub mod dotenv;
pub mod error;
pub mod header;
pub mod integration;
pub mod license;
pub mod model;
pub mod resolve;
pub mod secret;
pub mod store;
pub mod vault;

pub use config::{Config, RotationQueue, S3Location};
pub use crypto::KdfParams;
pub use error::{Error, Result};
pub use header::{DeviceInfo, DeviceKey, EnrollmentCode, RecoveryKey};
pub use integration::{Binding, Connection, ProviderLink, ProxyProfile, Pushed, Target};
pub use license::{License, LicenseStatus};
pub use model::{Entry, Environment, Project, Record, Secrets, Stamp, is_valid_var_name};
pub use secret::{Charset, Due, Generator, Policy, Secret, Version, VersionState};
pub use cache::CachedStore;
pub use store::{Cond, FolderStore, Put, Store};
pub use resolve::Resolver;
pub use vault::Vault;

/// Version of the core library, reported by the app and CLI.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
