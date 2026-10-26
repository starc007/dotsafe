use dotsafe_core::audit::{self, AuditAction, AuditEvent, AuditOutcome, RETENTION_DAYS};
use dotsafe_core::{FolderStore, KdfParams, Record, Vault};
use uuid::Uuid;

// Cheap Argon2 settings so tests run fast. Never use these for real vaults.
const FAST: KdfParams = KdfParams { m_cost_kib: 64, t_cost: 1, p_cost: 1 };
const DAY_MS: u64 = 24 * 60 * 60 * 1000;

fn event(at: u64, summary: &str) -> AuditEvent {
    AuditEvent {
        at,
        actor: "Claude Code".into(),
        action: AuditAction::Run,
        outcome: AuditOutcome::Allowed,
        summary: summary.into(),
        environment: "api/development".into(),
        variables: vec!["DATABASE_URL".into()],
        detail: Some("exit 0".into()),
    }
}

#[test]
fn events_from_every_device_come_back_newest_first() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::create(FolderStore::new(dir.path()), "pw", FAST).unwrap();
    let (laptop, desktop) = (Uuid::new_v4(), Uuid::new_v4());
    let today = 20_000 * DAY_MS;
    audit::record(&vault, laptop, event(today + 1, "bun test")).unwrap();
    audit::record(&vault, laptop, event(today + 3, "cargo test")).unwrap();
    audit::record(&vault, desktop, event(today + 2, "npm run dev")).unwrap();

    let other = Vault::unlock(FolderStore::new(dir.path()), "pw").unwrap();
    let summaries: Vec<String> = audit::recent(&other, 10).unwrap().into_iter().map(|e| e.summary).collect();
    assert_eq!(summaries, ["cargo test", "npm run dev", "bun test"]);
    assert_eq!(audit::recent(&other, 1).unwrap().len(), 1);

    // One record per device per day, sealed as record v3 like other new kinds.
    let audit_records = vault.list().unwrap().into_iter().filter(|r| matches!(r, Record::Audit(_))).count();
    assert_eq!(audit_records, 2);
}

#[test]
fn old_days_are_pruned() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::create(FolderStore::new(dir.path()), "pw", FAST).unwrap();
    let device = Uuid::new_v4();
    let long_ago = 10_000 * DAY_MS;
    audit::record(&vault, device, event(long_ago, "old")).unwrap();
    audit::record(&vault, device, event(long_ago + (RETENTION_DAYS + 2) * DAY_MS, "new")).unwrap();
    let summaries: Vec<String> = audit::recent(&vault, 10).unwrap().into_iter().map(|e| e.summary).collect();
    assert_eq!(summaries, ["new"]);
}
