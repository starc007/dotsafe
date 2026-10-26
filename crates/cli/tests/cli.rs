//! End-to-end tests that run the real `dotsafe` binary against a temp vault.
//! The OS keychain is disabled and the password comes from DOTSAFE_PASSWORD.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const PASSWORD: &str = "correct horse battery";

/// One "machine": its own config dir, sharing a vault folder with others.
struct Machine {
    config: tempfile::TempDir,
    vault: PathBuf,
}

impl Machine {
    fn new(vault: &Path) -> Machine {
        Machine { config: tempfile::tempdir().unwrap(), vault: vault.to_path_buf() }
    }

    fn cmd(&self, cwd: &Path, args: &[&str]) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_dotsafe"));
        cmd.current_dir(cwd)
            .args(args)
            .env("DOTSAFE_CONFIG_DIR", self.config.path())
            .env("DOTSAFE_NO_KEYCHAIN", "1")
            .env("DOTSAFE_PASSWORD", PASSWORD);
        cmd
    }

    fn run(&self, cwd: &Path, args: &[&str]) -> Output {
        self.cmd(cwd, args).output().unwrap()
    }

    fn ok(&self, cwd: &Path, args: &[&str]) -> String {
        let out = self.run(cwd, args);
        assert!(out.status.success(), "dotsafe {args:?} failed: {}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8(out.stdout).unwrap()
    }

    fn init(&self) -> String {
        self.ok(self.config.path(), &["init", "--path", self.vault.to_str().unwrap()])
    }
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn setup() -> (tempfile::TempDir, Machine, PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let machine = Machine::new(&root.path().join("vault"));
    let project = root.path().join("my-api");
    fs::create_dir(&project).unwrap();
    (root, machine, project)
}

#[test]
fn init_prints_a_recovery_key() {
    let (_root, m, _) = setup();
    let out = m.init();
    let key = out.trim();
    assert_eq!(key.split('-').count(), 13, "recovery key on stdout: {out:?}");
    assert!(m.vault.join("vault.json").exists());
}

#[test]
fn set_get_list_unset() {
    let (_root, m, project) = setup();
    m.init();
    m.ok(&project, &["set", "DATABASE_URL=postgres://localhost/db", "API_KEY=sk-123=="]);
    assert!(project.join(".dotsafe.json").exists(), "first write links the directory");

    assert_eq!(m.ok(&project, &["get", "API_KEY"]), "sk-123==\n");
    assert_eq!(m.ok(&project, &["list"]), "API_KEY\nDATABASE_URL\n");

    m.ok(&project, &["unset", "API_KEY"]);
    assert_eq!(m.ok(&project, &["list"]), "DATABASE_URL\n");
    let out = m.run(&project, &["get", "API_KEY"]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("API_KEY is not set in my-api/development"));
}

#[test]
fn environments_are_separate() {
    let (_root, m, project) = setup();
    m.init();
    m.ok(&project, &["set", "MODE=dev"]);
    m.ok(&project, &["set", "-e", "production", "MODE=prod"]);
    assert_eq!(m.ok(&project, &["get", "MODE"]), "dev\n");
    assert_eq!(m.ok(&project, &["get", "MODE", "--env", "production"]), "prod\n");
}

#[test]
fn subdirectories_use_the_parent_link() {
    let (_root, m, project) = setup();
    m.init();
    m.ok(&project, &["set", "A=1"]);
    let sub = project.join("src/deep");
    fs::create_dir_all(&sub).unwrap();
    assert_eq!(m.ok(&sub, &["get", "A"]), "1\n");
}

#[test]
fn import_dotenv_file() {
    let (_root, m, project) = setup();
    m.init();
    fs::write(
        project.join(".env"),
        "# local\nexport DB=postgres://x\nQUOTED=\"a b\\nc\"\nHOME_REF=$HOME\n",
    )
    .unwrap();
    let out = m.run(&project, &["import"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(stderr(&out).contains("Imported 3 variables"));
    assert_eq!(m.ok(&project, &["get", "QUOTED"]), "a b\nc\n");
    assert_eq!(m.ok(&project, &["get", "HOME_REF"]), "$HOME\n");
}

#[test]
fn run_injects_variables_and_forwards_exit_code() {
    let (_root, m, project) = setup();
    m.init();
    m.ok(&project, &["set", "GREETING=hello from dotsafe"]);
    let out = m.run(&project, &["run", "--", "sh", "-c", "echo \"$GREETING\"; exit 7"]);
    assert_eq!(String::from_utf8_lossy(&out.stdout), "hello from dotsafe\n");
    assert_eq!(out.status.code(), Some(7));
}

#[test]
fn run_without_double_dash_passes_flags_through() {
    let (_root, m, project) = setup();
    m.init();
    m.ok(&project, &["set", "X=1"]);
    let out = m.ok(&project, &["run", "sh", "-c", "echo $X"]);
    assert_eq!(out, "1\n");
}

#[test]
fn run_reports_missing_command() {
    let (_root, m, project) = setup();
    m.init();
    m.ok(&project, &["set", "X=1"]);
    let out = m.run(&project, &["run", "--", "definitely-not-a-real-command-xyz"]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("couldn't run definitely-not-a-real-command-xyz"));
}

#[test]
fn reads_in_unlinked_directory_explain_what_to_do() {
    let (_root, m, project) = setup();
    m.init();
    let out = m.run(&project, &["get", "A"]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("isn't linked to a project"));
}

#[test]
fn wrong_password_is_rejected() {
    let (_root, m, project) = setup();
    m.init();
    let out = m.cmd(&project, &["set", "A=1"]).env("DOTSAFE_PASSWORD", "nope").output().unwrap();
    assert!(!out.status.success());
    assert!(stderr(&out).contains("wrong password"));
}

#[test]
fn commands_before_init_point_to_init() {
    let (_root, m, project) = setup();
    let out = m.run(&project, &["list"]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("dotsafe init"));
}

#[test]
fn second_machine_connects_and_sees_the_same_secrets() {
    let (root, laptop, project) = setup();
    laptop.init();
    laptop.ok(&project, &["set", "SHARED=yes"]);

    // The desktop has the synced vault folder and the committed .dotsafe.json.
    let desktop = Machine::new(&root.path().join("vault"));
    let out = desktop.run(desktop.config.path(), &["init", "--path", desktop.vault.to_str().unwrap()]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(stderr(&out).contains("existing vault"));
    assert_eq!(desktop.ok(&project, &["get", "SHARED"]), "yes\n");

    desktop.ok(&project, &["set", "FROM_DESKTOP=1"]);
    assert_eq!(laptop.ok(&project, &["list"]), "FROM_DESKTOP\nSHARED\n");
}

#[test]
fn link_attaches_another_checkout_to_an_existing_project() {
    let (root, m, project) = setup();
    m.init();
    m.ok(&project, &["set", "A=1"]);
    let other = root.path().join("checkout-2");
    fs::create_dir(&other).unwrap();
    m.ok(&other, &["link", "my-api"]);
    assert_eq!(m.ok(&other, &["get", "A"]), "1\n");
}

#[test]
fn unlock_without_keychain_fails_cleanly() {
    let (_root, m, _project) = setup();
    m.init();
    let out = m.run(m.config.path(), &["unlock"]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("keychain"));
}

#[test]
fn invalid_variable_names_are_rejected() {
    let (_root, m, project) = setup();
    m.init();
    let out = m.run(&project, &["set", "1BAD=x"]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("invalid variable name"));
}

#[test]
fn shared_secrets_link_rotate_and_unlink() {
    let (root, m, api) = setup();
    m.init();
    let web = root.path().join("web");
    fs::create_dir(&web).unwrap();
    m.ok(&api, &["set", "STRIPE_KEY=sk_old", "PLAIN=1"]);
    m.ok(&web, &["set", "STRIPE_KEY=sk_old"]);

    let out = m.run(&api, &["secret", "share", "STRIPE_KEY", "--all"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(stderr(&out).contains("Linked 1 other environment"), "{}", stderr(&out));
    assert!(m.ok(&api, &["secrets", "list"]).contains("STRIPE_KEY\trotated today\t2 environments"));

    // Rotating changes every linked environment, and `run` sees it.
    m.ok(&api, &["secret", "rotate", "STRIPE_KEY", "--generate", "--note", "quarterly"]);
    let rotated = m.ok(&web, &["get", "STRIPE_KEY"]);
    assert_ne!(rotated.trim(), "sk_old");
    assert_eq!(m.ok(&api, &["get", "STRIPE_KEY"]), rotated);
    let injected = m.ok(&api, &["run", "--", "sh", "-c", "printf '%s\\n' \"$STRIPE_KEY\""]);
    assert_eq!(injected, rotated);

    // Linked variables can't be overwritten by accident.
    let out = m.run(&web, &["set", "STRIPE_KEY=oops"]);
    assert!(!out.status.success() && stderr(&out).contains("linked to a shared secret"));

    let history = m.ok(&api, &["secret", "history", "STRIPE_KEY"]);
    assert!(history.starts_with("1\tcurrent\ttoday\tquarterly\n2\tprevious\ttoday"), "{history}");
    assert!(!history.contains("sk_old"), "history never shows values");

    m.ok(&api, &["secret", "rollback", "STRIPE_KEY"]);
    assert_eq!(m.ok(&web, &["get", "STRIPE_KEY"]).trim(), "sk_old");

    // An unlinked copy keeps its value when the secret changes.
    m.ok(&web, &["secret", "unlink", "STRIPE_KEY"]);
    m.ok(&api, &["secret", "rotate", "STRIPE_KEY", "--generate", "--leaked"]);
    assert_eq!(m.ok(&web, &["get", "STRIPE_KEY"]).trim(), "sk_old");
    assert_ne!(m.ok(&api, &["get", "STRIPE_KEY"]).trim(), "sk_old");
    assert!(m.ok(&api, &["secret", "history", "STRIPE_KEY"]).contains("current\ttoday\tLeaked"));
}

#[test]
fn rotation_policies_flag_due_secrets() {
    let (_root, m, api) = setup();
    m.init();
    m.ok(&api, &["set", "TOKEN=t"]);
    m.ok(&api, &["secret", "share", "TOKEN"]);
    let out = m.run(&api, &["secret", "due"]);
    assert!(stderr(&out).contains("Nothing is due"));
    m.ok(&api, &["secret", "policy", "TOKEN", "--every", "3"]);
    // Rotated today with a 3-day policy: due within a week.
    assert!(m.ok(&api, &["secret", "due"]).starts_with("TOKEN\tdue soon"));
}
