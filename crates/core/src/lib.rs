//! Shared core for dotsafe: vault format, encryption and sync.
//!
//! Both the desktop app and the CLI depend on this crate so they read and
//! write the same vault on disk.

/// Version of the core library, reported by the app and CLI.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
