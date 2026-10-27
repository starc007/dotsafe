//! A macOS notification when shared secrets are due for rotation, while the
//! vault is unlocked. At most once a day, unless more become due.
//!
//! The notification says how many, not which: secret names would otherwise
//! sit in Notification Center and on the lock screen.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use dotsafe_core::{Due, Record, Secrets};
use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

use crate::state::AppState;

const FIRST_CHECK: Duration = Duration::from_secs(60);
const INTERVAL: Duration = Duration::from_secs(30 * 60);
const DAY_MS: u64 = 24 * 60 * 60 * 1000;

/// How many of the user's shared secrets are overdue or due soon.
/// Connection tokens are left out; they're managed from Integrations.
pub fn due_count(records: &[Record], now_ms: u64) -> usize {
    let tokens = dotsafe_core::integration::credentials(records);
    Secrets::from_records(records)
        .iter()
        .filter(|s| !tokens.contains(&s.id) && matches!(s.due(now_ms), Due::Overdue | Due::Soon))
        .count()
}

/// Whether to notify about `count` now, given the last notification as
/// (day, count): on a new day, or when more became due the same day.
pub fn should_notify(count: usize, today: u64, last: Option<(u64, usize)>) -> bool {
    count > 0 && last.is_none_or(|(day, before)| day != today || count > before)
}

pub fn spawn_reminders(state: AppState, app: AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(FIRST_CHECK);
        let mut last: Option<(u64, usize)> = None;
        loop {
            // The shared vault, used off the session lock.
            let vault = state.0.lock().ok().and_then(|s| s.vault.clone());
            if let Some(vault) = vault
                && let Ok(records) = vault.list()
            {
                let now = now_ms();
                let count = due_count(&records, now);
                if should_notify(count, now / DAY_MS, last) {
                    let body = match count {
                        1 => "1 shared secret is due for rotation.".to_string(),
                        n => format!("{n} shared secrets are due for rotation."),
                    };
                    let shown = app.notification().builder().title("dotsafe").body(body).show();
                    if shown.is_ok() {
                        last = Some((now / DAY_MS, count));
                    }
                }
            }
            std::thread::sleep(INTERVAL);
        }
    });
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use dotsafe_core::{Policy, Secret, Stamp};
    use uuid::Uuid;

    #[test]
    fn counts_due_secrets_but_not_connection_tokens() {
        let device = Uuid::new_v4();
        let old = Stamp::new(0, device);
        let mut due = Secret::new("STRIPE", "x", old);
        due.set_policy(Policy { rotate_every_days: Some(30), ..Policy::default() }, old);
        let mut token = Secret::new("Acme admin token", "y", old);
        token.set_policy(Policy { rotate_every_days: Some(30), ..Policy::default() }, old);
        let fresh = Secret::new("NEW", "z", Stamp::new(100 * DAY_MS, device));
        let connection = dotsafe_core::Connection::new("openai", "Acme", Some(token.id), Default::default(), old);
        let records = vec![Record::Secret(due), Record::Secret(token), Record::Secret(fresh), Record::Connection(connection)];
        assert_eq!(due_count(&records, 100 * DAY_MS), 1);
    }

    #[test]
    fn notifies_once_a_day_unless_more_become_due() {
        assert!(!should_notify(0, 5, None));
        assert!(should_notify(2, 5, None));
        assert!(!should_notify(2, 5, Some((5, 2))));
        assert!(should_notify(3, 5, Some((5, 2))), "one more became due");
        assert!(should_notify(2, 6, Some((5, 2))), "a new day");
    }
}
