use std::fs;
use std::path::Path;

use dotsafe_core::{Environment, Error, FolderStore, KdfParams, Project, Record, Resolver, Secret, Vault};
use serde_json::Value;
use uuid::Uuid;

// Cheap Argon2 settings so tests run fast. Never use these for real vaults.
const FAST: KdfParams = KdfParams { m_cost_kib: 64, t_cost: 1, p_cost: 1 };

fn record_json(root: &Path, id: Uuid) -> Value {
    serde_json::from_slice(&fs::read(root.join("records").join(format!("{id}.json"))).unwrap()).unwrap()
}

struct Setup {
    dir: tempfile::TempDir,
    vault: Vault,
    device: Uuid,
    project: Project,
}

fn setup() -> Setup {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::create(FolderStore::new(dir.path()), "pw", FAST).unwrap();
    let device = Uuid::new_v4();
    let project = Project::new("api", vault.stamp(device));
    vault.put(Record::Project(project.clone())).unwrap();
    Setup { dir, vault, device, project }
}

impl Setup {
    fn env(&self, name: &str, vars: &[(&str, &str)], parent: Option<Uuid>) -> Environment {
        let mut env = Environment::new(self.project.id, name, self.vault.stamp(self.device));
        env.parent = parent;
        for (k, v) in vars {
            env.set(*k, *v, self.vault.stamp(self.device));
        }
        self.vault.put(Record::Environment(env.clone())).unwrap();
        env
    }

    fn resolver(&self) -> Resolver {
        Resolver::from_records(&self.vault.list().unwrap())
    }

    fn resolved(&self, env: Uuid) -> Vec<(String, String)> {
        self.resolver().resolve(env).unwrap()
    }
}

fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
    list.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

#[test]
fn references_expand_including_nested_and_linked_ones() {
    let s = setup();
    let secret = Secret::new("db password", "hunter2", s.vault.stamp(s.device));
    s.vault.put(Record::Secret(secret.clone())).unwrap();
    let mut env = s.env(
        "development",
        &[
            ("DB_HOST", "localhost"),
            ("DB_URL", "postgres://app:${{ DB_PASSWORD }}@${{DB_HOST}}/app"),
            ("API", "${{ BASE }}/v1"),
            ("BASE", "https://${{ DB_HOST }}"),
            ("LITERAL", "keep $${{ THIS }} as is"),
        ],
        None,
    );
    env.link("DB_PASSWORD", secret.id, s.vault.stamp(s.device));
    s.vault.put(Record::Environment(env.clone())).unwrap();

    assert_eq!(
        s.resolved(env.id),
        pairs(&[
            ("API", "https://localhost/v1"),
            ("BASE", "https://localhost"),
            ("DB_HOST", "localhost"),
            ("DB_PASSWORD", "hunter2"),
            ("DB_URL", "postgres://app:hunter2@localhost/app"),
            ("LITERAL", "keep ${{ THIS }} as is"),
        ])
    );
    assert_eq!(record_json(s.dir.path(), env.id)["version"], 5, "references need v5");
}

#[test]
fn broken_references_are_errors_not_wrong_values() {
    let s = setup();
    for (vars, why) in [
        (vec![("A", "${{ MISSING }}")], "A refers to MISSING, which isn't set"),
        (vec![("A", "${{ B }}"), ("B", "${{ A }}")], "loop"),
        (vec![("A", "${{ not a name }}")], "isn't a variable name"),
        (vec![("A", "${{ B")], "no closing"),
    ] {
        let env = s.env(&format!("env-{}", Uuid::new_v4()), &vars, None);
        let err = s.resolver().resolve(env.id).unwrap_err();
        assert!(matches!(&err, Error::Reference(m) if m.contains(why)), "{vars:?}: {err}");
    }
}

#[test]
fn children_inherit_override_and_remove() {
    let s = setup();
    let production = s.env(
        "production",
        &[("DB_HOST", "db.prod"), ("DB_URL", "postgres://${{ DB_HOST }}/app"), ("SENTRY_DSN", "https://sentry"), ("LOG", "warn")],
        None,
    );
    let mut staging = s.env("staging", &[("DB_HOST", "db.staging"), ("FEATURE", "on")], Some(production.id));
    staging.hide("SENTRY_DSN", s.vault.stamp(s.device));
    s.vault.put(Record::Environment(staging.clone())).unwrap();

    // References use the final values: staging's DB_URL points at staging's host.
    assert_eq!(
        s.resolved(staging.id),
        pairs(&[("DB_HOST", "db.staging"), ("DB_URL", "postgres://db.staging/app"), ("FEATURE", "on"), ("LOG", "warn")])
    );
    let from: Vec<(String, bool)> =
        s.resolver().entries(staging.id).unwrap().into_iter().map(|e| (e.key, e.from == staging.id)).collect();
    assert_eq!(
        from,
        [("DB_HOST".into(), true), ("DB_URL".into(), false), ("FEATURE".into(), true), ("LOG".into(), false)]
    );

    // Deleting staging's own value lets production's show through again.
    let Some(Record::Environment(mut stored)) = s.vault.get(staging.id).unwrap() else { panic!() };
    stored.remove("DB_HOST", s.vault.stamp(s.device));
    s.vault.put(Record::Environment(stored)).unwrap();
    assert!(s.resolved(staging.id).contains(&("DB_HOST".into(), "db.prod".into())));
    assert_eq!(record_json(s.dir.path(), staging.id)["version"], 5);
    assert_eq!(s.resolved(production.id).len(), 4, "the parent is unaffected");
}

#[test]
fn broken_parent_chains_are_errors() {
    let s = setup();
    let a_id = Environment::id_for(s.project.id, "a");
    let b = s.env("b", &[], Some(a_id));
    s.env("a", &[], Some(b.id));
    let err = s.resolver().resolve(b.id).unwrap_err();
    assert!(err.to_string().contains("loop of environments"), "{err}");

    let orphan = s.env("orphan", &[("X", "1")], Some(Uuid::new_v4()));
    assert!(s.resolver().resolve(orphan.id).unwrap_err().to_string().contains("deleted"));

    let other = Project::new("web", s.vault.stamp(s.device));
    let foreign = Environment::new(other.id, "production", s.vault.stamp(s.device));
    s.vault.put(Record::Environment(foreign.clone())).unwrap();
    let crossing = s.env("crossing", &[], Some(foreign.id));
    assert!(s.resolver().resolve(crossing.id).unwrap_err().to_string().contains("its own project"));
}

#[test]
fn plain_environments_stay_readable_by_older_clients() {
    let s = setup();
    let plain = s.env("development", &[("PORT", "3000"), ("PRICE", "$5 and ${HOME}")], None);
    assert_eq!(record_json(s.dir.path(), plain.id)["version"], 2, "plain ${{VAR}}-free values don't need v5");
}

#[test]
fn v5_records_cant_be_relabelled_for_older_clients() {
    let s = setup();
    let env = s.env("staging", &[("A", "${{ B }}"), ("B", "1")], None);
    let path = s.dir.path().join("records").join(format!("{}.json", env.id));
    let mut json = record_json(s.dir.path(), env.id);
    json["version"] = 4.into();
    fs::write(&path, json.to_string()).unwrap();
    assert!(matches!(s.vault.get(env.id), Err(Error::RecordTampered(_))));
}
