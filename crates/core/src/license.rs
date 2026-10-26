//! A license activated on this machine, as saved in the config. Checking it
//! with the license server happens in the app; this is only the record and
//! what it means while offline.

use serde::{Deserialize, Serialize};

/// After this long without reaching the license server, the license shows as
/// unverified. It's a nudge to go online, not a lockout.
pub const OFFLINE_GRACE_SECS: u64 = 30 * 24 * 60 * 60;
/// Recheck an active license in the background about this often.
pub const RECHECK_SECS: u64 = 12 * 60 * 60;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct License {
    pub key: String,
    /// This activation's id on the license server, needed to validate it and
    /// to free the device slot later.
    pub instance_id: String,
    pub device_name: String,
    /// Unix seconds.
    pub activated_at: u64,
    /// Unix seconds of the last time the server confirmed the license.
    pub checked_at: u64,
    /// The server said the license is no longer valid (refunded or disabled).
    #[serde(default)]
    pub revoked: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LicenseStatus {
    Active,
    /// Not confirmed for longer than the offline grace period.
    Unverified,
    Revoked,
}

impl License {
    pub fn status(&self, now: u64) -> LicenseStatus {
        if self.revoked {
            LicenseStatus::Revoked
        } else if now.saturating_sub(self.checked_at) > OFFLINE_GRACE_SECS {
            LicenseStatus::Unverified
        } else {
            LicenseStatus::Active
        }
    }

    /// Whether a background check is due. Revoked licenses are still
    /// rechecked, in case the key was re-enabled.
    pub fn needs_check(&self, now: u64) -> bool {
        now.saturating_sub(self.checked_at) >= RECHECK_SECS
    }
}

pub fn unix_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: u64 = 24 * 60 * 60;

    fn license(checked_at: u64) -> License {
        License {
            key: "K".into(),
            instance_id: "lki_1".into(),
            device_name: "Mac".into(),
            activated_at: 0,
            checked_at,
            revoked: false,
        }
    }

    #[test]
    fn stays_active_through_the_offline_grace_period() {
        let l = license(1_000 * DAY);
        assert_eq!(l.status(1_000 * DAY), LicenseStatus::Active);
        assert_eq!(l.status(1_030 * DAY), LicenseStatus::Active);
        assert_eq!(l.status(1_031 * DAY), LicenseStatus::Unverified);
        // A clock set backwards doesn't break anything.
        assert_eq!(l.status(10 * DAY), LicenseStatus::Active);
    }

    #[test]
    fn revoked_wins() {
        let l = License { revoked: true, ..license(100 * DAY) };
        assert_eq!(l.status(100 * DAY), LicenseStatus::Revoked);
    }

    #[test]
    fn rechecks_twice_a_day() {
        let l = license(100 * DAY);
        assert!(!l.needs_check(100 * DAY + 11 * 60 * 60));
        assert!(l.needs_check(100 * DAY + 12 * 60 * 60));
    }

    #[test]
    fn older_records_without_revoked_still_load() {
        let json = r#"{"key":"K","instance_id":"i","device_name":"d","activated_at":1,"checked_at":2}"#;
        let l: License = serde_json::from_str(json).unwrap();
        assert!(!l.revoked);
    }
}
