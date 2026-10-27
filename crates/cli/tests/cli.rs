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

#[test]
fn run_with_proxy_keeps_known_keys_out_of_the_process() {
    let (_root, m, api) = setup();
    m.init();
    m.ok(&api, &["set", "OPENAI_API_KEY=sk-real-key", "PLAIN=1"]);
    let out = m.run(&api, &["run", "--proxy", "--", "sh", "-c", "printf '%s|%s|%s' \"$OPENAI_API_KEY\" \"$OPENAI_BASE_URL\" \"$PLAIN\""]);
    assert!(out.status.success(), "{}", stderr(&out));
    let seen = String::from_utf8(out.stdout.clone()).unwrap();
    let parts: Vec<&str> = seen.split('|').collect();
    assert_eq!(parts[0], "dotsafe-proxied");
    assert!(parts[1].starts_with("http://127.0.0.1:") && parts[1].ends_with("/v1"), "{seen}");
    assert_eq!(parts[2], "1", "other variables are injected as usual");
    assert!(!seen.contains("sk-real-key"));
    assert!(stderr(&out).contains("OPENAI_API_KEY go through a local proxy"));

    // Exit codes come through.
    let out = m.run(&api, &["run", "--proxy", "--", "sh", "-c", "exit 7"]);
    assert_eq!(out.status.code(), Some(7));
}

#[test]
fn audit_starts_empty() {
    let (_root, m, api) = setup();
    m.init();
    let out = m.run(&api, &["audit"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(stderr(&out).contains("No agent activity yet."));
}

#[test]
fn device_list_starts_empty_and_remove_needs_a_known_name() {
    let (_root, m, api) = setup();
    m.init();
    let out = m.run(&api, &["device", "list"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(stderr(&out).contains("No devices use their own key yet"));
    let out = m.run(&api, &["device", "remove", "Ghost"]);
    assert!(!out.status.success() && stderr(&out).contains("no device named"));
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .current_dir(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn export_prints_and_writes_env_files() {
    let (_root, m, project) = setup();
    m.init();
    m.ok(&project, &["set", "PLAIN=abc", "SPACED=two words", "DOLLAR=pa$$"]);
    let printed = m.ok(&project, &["export"]);
    assert_eq!(printed, "DOLLAR='pa$$'\nPLAIN=abc\nSPACED='two words'\n");

    // In a checkout, the file must be ignored first.
    git(&project, &["init", "-q"]);
    let out = m.run(&project, &["export", "-o", ".env"]);
    assert!(!out.status.success() && stderr(&out).contains(".gitignore"), "{}", stderr(&out));
    fs::write(project.join(".gitignore"), ".env\n").unwrap();
    m.ok(&project, &["export", "-o", ".env"]);
    let written = fs::read_to_string(project.join(".env")).unwrap();
    assert!(written.starts_with("# Written by `dotsafe export` from my-api/development."));
    assert!(written.ends_with(&printed));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(fs::metadata(project.join(".env")).unwrap().permissions().mode() & 0o777, 0o600);
    }

    // Its own file is refreshed; anyone else's is left alone.
    m.ok(&project, &["set", "PLAIN=changed"]);
    m.ok(&project, &["export", "-o", ".env"]);
    assert!(fs::read_to_string(project.join(".env")).unwrap().contains("PLAIN=changed"));
    fs::write(project.join(".env.local"), "MINE=1\n").unwrap();
    fs::write(project.join(".gitignore"), ".env*\n").unwrap();
    let out = m.run(&project, &["export", "-o", ".env.local"]);
    assert!(!out.status.success() && stderr(&out).contains("didn't write it"));
    assert_eq!(fs::read_to_string(project.join(".env.local")).unwrap(), "MINE=1\n");
}

#[test]
fn scan_finds_vault_values_without_printing_them() {
    let (_root, m, project) = setup();
    m.init();
    m.ok(&project, &["set", "STRIPE_KEY=sk_live_scanme123", "PORT=3000"]);
    m.ok(&project, &["set", "SHARED=tok_shared_987654"]);
    m.ok(&project, &["secret", "share", "SHARED"]);
    fs::write(project.join("clean.js"), "listen(3000)\n").unwrap();
    assert!(m.run(&project, &["scan"]).status.success());

    fs::create_dir(project.join("src")).unwrap();
    fs::write(project.join("src/config.js"), "const a = 1;\n  key: 'sk_live_scanme123',\n").unwrap();
    fs::write(project.join("notes.txt"), "dG9rX3NoYXJlZF85ODc2NTQ=\n").unwrap();
    let out = m.run(&project, &["scan"]);
    assert_eq!(out.status.code(), Some(1));
    let report = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(report.contains("config.js:2:9\tmy-api/development/STRIPE_KEY"), "{report}");
    assert!(report.contains("notes.txt:1:1\tsecret SHARED"), "base64 is caught too: {report}");
    assert!(!report.contains("sk_live") && !stderr(&out).contains("sk_live"), "values are never printed");
}

#[test]
fn scan_staged_and_the_pre_commit_hook() {
    let (_root, m, project) = setup();
    m.init();
    m.ok(&project, &["set", "API_KEY=sk_live_hooked42"]);
    git(&project, &["init", "-q"]);
    m.ok(&project, &["scan", "--install-hook"]);
    let hook = fs::read_to_string(project.join(".git/hooks/pre-commit")).unwrap();
    assert!(hook.contains("dotsafe scan --staged"));

    fs::write(project.join("a.txt"), "API_KEY=sk_live_hooked42\n").unwrap();
    fs::write(project.join("b.txt"), "fine\n").unwrap();
    git(&project, &["add", "b.txt"]);
    assert!(m.run(&project, &["scan", "--staged"]).status.success(), "unstaged files aren't checked");
    git(&project, &["add", "a.txt"]);
    let out = m.run(&project, &["scan", "--staged"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stdout).contains("a.txt:1:9\tmy-api/development/API_KEY"));
}

#[test]
fn connections_providers_and_targets() {
    let (_root, m, project) = setup();
    m.init();
    m.ok(&project, &["set", "OPENAI_API_KEY=sk-proj-1"]);
    m.ok(&project, &["secret", "share", "OPENAI_API_KEY"]);
    let connect = |args: &[&str]| {
        let mut cmd = m.cmd(&project, args);
        cmd.stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
        let mut child = cmd.spawn().unwrap();
        use std::io::Write;
        child.stdin.take().unwrap().write_all(b"admin-token-123\n").unwrap();
        child.wait_with_output().unwrap()
    };

    // Settings are checked against the service.
    let out = connect(&["connect", "openai", "--token-stdin"]);
    assert!(!out.status.success() && stderr(&out).contains("Project ID is required"), "{}", stderr(&out));
    assert!(!connect(&["connect", "heroku", "--token-stdin"]).status.success());
    let out = connect(&["connect", "openai", "--name", "Acme", "--set", "project_id=proj_1", "--token-stdin"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let out = connect(&["connect", "vercel", "--name", "Web", "--token-stdin"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(m.ok(&project, &["connections"]), "Acme\tOpenAI\tproject_id=proj_1\nWeb\tVercel\t\n");
    // The token is stored but never printed, and stays out of the secrets list.
    assert!(!m.ok(&project, &["secret", "list"]).contains("admin token"));
    assert!(!m.ok(&project, &["connections"]).contains("admin-token-123"));
    let mut cmd = m.cmd(&project, &["reconnect", "Acme", "--token-stdin"]);
    cmd.stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
    let mut child = cmd.spawn().unwrap();
    {
        use std::io::Write;
        child.stdin.take().unwrap().write_all(b"admin-token-456\n").unwrap();
    }
    assert!(child.wait_with_output().unwrap().status.success());
    let history = m.ok(&project, &["secret", "history", "Acme admin token"]);
    assert!(history.starts_with("1\tcurrent") && history.contains("retired"), "{history}");

    // A provider must be a provider and a target a target.
    let wrong = m.run(&project, &["secret", "provider", "OPENAI_API_KEY", "--connection", "Web"]);
    assert!(stderr(&wrong).contains("deploy target, not a provider"));
    m.ok(&project, &["secret", "provider", "OPENAI_API_KEY", "--connection", "Acme", "--key-id", "svc_1"]);
    let wrong = m.run(&project, &["secret", "target", "add", "OPENAI_API_KEY", "--connection", "Acme"]);
    assert!(stderr(&wrong).contains("provider, not a deploy target"));
    let missing = m.run(&project, &["secret", "target", "add", "OPENAI_API_KEY", "--connection", "Web"]);
    assert!(stderr(&missing).contains("Project is required"), "{}", stderr(&missing));
    m.ok(&project, &["secret", "target", "add", "OPENAI_API_KEY", "--connection", "Web", "--set", "project=web"]);
    assert_eq!(m.ok(&project, &["secret", "target", "list", "OPENAI_API_KEY"]), "1\tWeb\tOPENAI_API_KEY\tproject=web\tnever pushed\n");

    // Connections in use can't be removed until nothing uses them.
    assert!(stderr(&m.run(&project, &["disconnect", "Web"])).contains("secrets still use Web"));
    m.ok(&project, &["secret", "target", "remove", "OPENAI_API_KEY", "1"]);
    m.ok(&project, &["disconnect", "Web"]);
    m.ok(&project, &["secret", "provider", "OPENAI_API_KEY", "--none"]);
    m.ok(&project, &["disconnect", "Acme"]);
    assert_eq!(m.ok(&project, &["connections"]), "");
    assert!(!m.ok(&project, &["secret", "list"]).contains("admin token"), "the stored tokens are deleted");
}

#[test]
fn rotation_queue_lists_what_a_removed_device_could_read() {
    let (_root, m, project) = setup();
    m.init();
    m.ok(&project, &["set", "DB_URL=postgres://old", "TOKEN=t1"]);
    m.ok(&project, &["secret", "share", "TOKEN"]);
    assert!(stderr(&m.run(&project, &["secret", "queue"])).contains("Nothing to rotate"));

    // As if a device was removed just now (`device remove` needs an enrolled device).
    std::thread::sleep(std::time::Duration::from_millis(20));
    let path = m.config.path().join("config.json");
    let mut config: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64;
    config["rotation_queue"] = serde_json::json!({ "since": now, "device": "Lost laptop" });
    fs::write(&path, serde_json::to_vec(&config).unwrap()).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));

    let out = m.run(&project, &["secret", "queue"]);
    assert_eq!(String::from_utf8_lossy(&out.stdout), "secret\tTOKEN\trotate by hand\nvariable\tmy-api/development/DB_URL\n");
    assert!(stderr(&out).contains("2 values to rotate since removing Lost laptop"));

    m.ok(&project, &["set", "DB_URL=postgres://new"]);
    assert_eq!(m.ok(&project, &["secret", "queue"]), "secret\tTOKEN\trotate by hand\n");
    m.ok(&project, &["secret", "queue", "--dismiss"]);
    assert!(stderr(&m.run(&project, &["secret", "queue"])).contains("Nothing to rotate"));
}

#[test]
fn guided_and_aws_providers_from_the_cli() {
    let (_root, m, project) = setup();
    m.init();
    m.ok(&project, &["set", "STRIPE_KEY=sk_live_1", "AWS_SECRET_ACCESS_KEY=s1", "AWS_ACCESS_KEY_ID=AKIA1"]);
    for key in ["STRIPE_KEY", "AWS_SECRET_ACCESS_KEY", "AWS_ACCESS_KEY_ID"] {
        m.ok(&project, &["secret", "share", key]);
    }

    // Stripe needs no token: nothing is asked for or stored.
    let out = m.run(&project, &["connect", "stripe"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(!stderr(&out).contains("stored"), "{}", stderr(&out));
    m.ok(&project, &["secret", "provider", "STRIPE_KEY", "--connection", "Stripe"]);
    // A by-hand rotation, then the old key can only be retired by hand.
    let mut rotate = m.cmd(&project, &["secret", "rotate", "STRIPE_KEY", "--generate"]);
    assert!(rotate.output().unwrap().status.success());
    let refused = m.run(&project, &["secret", "retire", "STRIPE_KEY"]);
    assert!(stderr(&refused).contains("only be revoked in its dashboard"), "{}", stderr(&refused));
    assert!(stderr(&m.run(&project, &["secret", "retire", "STRIPE_KEY", "--by-hand"])).contains("Marked the previous key"));

    // AWS's key-ID setting names a shared secret.
    let mut cmd = m.cmd(&project, &["connect", "aws_iam", "--name", "AWS", "--set", "access_key_id=AKIAADMIN", "--token-stdin"]);
    cmd.stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
    let mut child = cmd.spawn().unwrap();
    {
        use std::io::Write;
        child.stdin.take().unwrap().write_all(b"admin-secret\n").unwrap();
    }
    assert!(child.wait_with_output().unwrap().status.success());
    let missing = m.run(
        &project,
        &["secret", "provider", "AWS_SECRET_ACCESS_KEY", "--connection", "AWS", "--set", "user_name=ci", "--set", "access_key_id_secret=NOPE"],
    );
    assert!(stderr(&missing).contains("no shared secret named \"NOPE\""), "{}", stderr(&missing));
    m.ok(
        &project,
        &["secret", "provider", "AWS_SECRET_ACCESS_KEY", "--connection", "AWS", "--set", "user_name=ci", "--set", "access_key_id_secret=AWS_ACCESS_KEY_ID"],
    );
}

#[test]
fn import_reads_other_managers_exports() {
    let (root, m, project) = setup();
    m.init();
    fs::write(project.join("infisical.json"), r#"[{"key": "API_KEY", "value": "sk_1", "type": "shared"}]"#).unwrap();
    m.ok(&project, &["import", "infisical.json"]);
    assert_eq!(m.ok(&project, &["get", "API_KEY"]), "sk_1\n");

    // `--from doppler` runs Doppler's CLI; a stand-in prints what it would.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let bin = root.path().join("bin");
        fs::create_dir(&bin).unwrap();
        let script = bin.join("doppler");
        fs::write(&script, "#!/bin/sh\necho '{\"DB_URL\": \"postgres://d\", \"DOPPLER_PROJECT\": \"api\"}'\n").unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default());
        let out = m.cmd(&project, &["import", "--from", "doppler"]).env("PATH", path).output().unwrap();
        assert!(out.status.success(), "{}", stderr(&out));
        assert!(stderr(&out).contains("Imported 1 variables from Doppler"), "{}", stderr(&out));
        assert_eq!(m.ok(&project, &["list"]), "API_KEY\nDB_URL\n", "Doppler's own variables are left out");
    }

    let missing = m.cmd(&project, &["import", "--from", "infisical"]).env("PATH", "/nonexistent").output().unwrap();
    assert!(stderr(&missing).contains("infisical isn't installed"), "{}", stderr(&missing));
    assert!(!m.run(&project, &["import", "--from", "1password"]).status.success(), "1Password needs --item");
}

#[test]
fn custom_proxy_profiles_keep_more_keys_out_of_processes() {
    let (_root, m, project) = setup();
    m.init();
    m.ok(&project, &["set", "MISTRAL_API_KEY=m-real-key"]);
    let bad = m.run(&project, &["proxy", "add", "Evil", "--key-var", "MISTRAL_API_KEY", "--base-url-var", "MISTRAL_BASE_URL", "--upstream", "http://evil.example"]);
    assert!(stderr(&bad).contains("https://"), "{}", stderr(&bad));
    let taken = m.run(&project, &["proxy", "add", "Mine", "--key-var", "OPENAI_API_KEY", "--base-url-var", "X_URL", "--upstream", "https://x.example"]);
    assert!(stderr(&taken).contains("already goes through the proxy"), "{}", stderr(&taken));
    m.ok(
        &project,
        &["proxy", "add", "Mistral", "--key-var", "MISTRAL_API_KEY", "--base-url-var", "MISTRAL_BASE_URL", "--upstream", "https://api.mistral.ai", "--base-path", "/v1"],
    );
    let list = m.ok(&project, &["proxy", "list"]);
    assert!(list.contains("Mistral\tMISTRAL_API_KEY\thttps://api.mistral.ai\tcustom"), "{list}");

    let seen = m.ok(&project, &["run", "--proxy", "--", "sh", "-c", "printf '%s %s' \"$MISTRAL_API_KEY\" \"$MISTRAL_BASE_URL\""]);
    let (key, url) = seen.split_once(' ').unwrap();
    assert_eq!(key, "dotsafe-proxied");
    assert!(url.starts_with("http://127.0.0.1:") && url.ends_with("/v1"), "{url}");
    m.ok(&project, &["proxy", "remove", "Mistral"]);
    assert_eq!(m.ok(&project, &["run", "--proxy", "--", "sh", "-c", "printf '%s' \"$MISTRAL_API_KEY\""]), "m-real-key");
}
