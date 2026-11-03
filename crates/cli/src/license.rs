//! `dotsafe license`: activate, show and remove this machine's license. dotsafe
//! is paid, so every other command needs one first. The license is shared with
//! the app (it's `license.json` in the config directory), so activating in
//! either is enough.

use anyhow::{Result, bail};
use clap::Subcommand;
use dotsafe_core::license::unix_now;
use dotsafe_core::{License, LicenseStatus};
use dotsafe_integrations::licensing::{self, ApiError};

#[derive(Subcommand)]
pub enum LicenseCmd {
    /// Activate a license key on this machine
    Activate {
        /// The key from your purchase email
        key: String,
    },
    /// Show the license on this machine
    Status,
    /// Free this machine's activation so the key can be used elsewhere
    Remove {
        /// Forget the key here even if the license server can't be reached
        #[arg(long)]
        force: bool,
    },
}

/// Stops any command but `license` until this machine has a license.
pub fn require() -> Result<()> {
    if License::load()?.is_none() {
        bail!(
            "dotsafe needs a license on this machine. Activate it in the app, or run `dotsafe license activate <key>`. Buy one at https://dotsafe.dev/pricing"
        );
    }
    Ok(())
}

pub fn run(cmd: LicenseCmd) -> Result<()> {
    match cmd {
        LicenseCmd::Activate { key } => {
            licensing::activate_here(&key)?.save()?;
            eprintln!("License activated on this machine.");
        }
        LicenseCmd::Status => match License::load()? {
            None => eprintln!("No license on this machine. Run `dotsafe license activate <key>`."),
            Some(license) => {
                let status = match license.status(unix_now()) {
                    LicenseStatus::Active => "active",
                    LicenseStatus::Unverified => "not verified in a while; go online so dotsafe can check it",
                    LicenseStatus::Revoked => "no longer valid (refunded or disabled)",
                };
                println!("{} on {}: {status}", mask(&license.key), license.device_name);
            }
        },
        LicenseCmd::Remove { force } => {
            let Some(license) = License::load()? else {
                eprintln!("No license on this machine.");
                return Ok(());
            };
            match licensing::release(&license, force) {
                Ok(true) => {}
                Ok(false) => eprintln!("Couldn't reach the license server; this machine still counts toward the key's limit."),
                Err(ApiError::Offline) => bail!("{}; add --force to remove it anyway", licensing::OFFLINE_MESSAGE),
                Err(e) => return Err(e.into()),
            }
            License::remove()?;
            eprintln!("License removed from this machine.");
        }
    }
    Ok(())
}

/// The key with all but its last group hidden.
fn mask(key: &str) -> String {
    let last = key.rsplit('-').next().unwrap_or(key);
    let tail: String = last.chars().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect();
    format!("••••-{tail}")
}
