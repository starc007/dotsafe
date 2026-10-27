//! Desktop commands, called through real IPC on Tauri's mock runtime: the
//! same command names, camelCase arguments and JSON the UI sends, against a
//! vault in a temp folder.
//!
//! Copying is checked through the hash the session keeps; test builds don't
//! touch the system clipboard. The file picker, and commands that reach the
//! network or the keychain, aren't covered here. Every test gets its own `AppState`; the config directory is shared
//! and only ever written, never read back.

use std::path::Path;
use std::sync::{Arc, Once};

use dotsafe_core::{Config, FolderStore, KdfParams, Vault};
use serde_json::{Value, json};
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{INVOKE_KEY, MockRuntime, get_ipc_response, mock_builder, mock_context, noop_assets};
use tauri::webview::InvokeRequest;
use tauri::{App, WebviewWindow, WebviewWindowBuilder};

use crate::state::{AppState, Session};
use crate::{broker, commands, devices, integrations, secrets, storage};

// Cheap Argon2 settings so tests run fast. Never use these for real vaults.
const FAST: KdfParams = KdfParams { m_cost_kib: 64, t_cost: 1, p_cost: 1 };
const PASSWORD: &str = "correct horse battery";

/// One app window and the session behind it.
struct Desktop {
    webview: WebviewWindow<MockRuntime>,
    state: AppState,
    _app: App<MockRuntime>,
}

impl Desktop {
    fn new() -> Desktop {
        static CONFIG_DIR: Once = Once::new();
        CONFIG_DIR.call_once(|| {
            let dir = std::env::temp_dir().join(format!("dotsafe-desktop-tests-{}", std::process::id()));
            // SAFETY: runs once, before any test reads the environment.
            unsafe { std::env::set_var("DOTSAFE_CONFIG_DIR", dir) };
        });
        let state = AppState(Arc::new(std::sync::Mutex::new(Session { config: None, ..Session::default() })));
        let app = mock_builder()
            .manage(state.clone())
            .manage(broker::Approvals::default())
            .invoke_handler(tauri::generate_handler![
                commands::status,
                commands::folder_has_vault,
                commands::create_vault,
                commands::connect_vault,
                commands::bucket_has_vault,
                commands::unlock,
                commands::unlock_with_recovery,
                commands::change_password,
                commands::reset_recovery_key,
                commands::lock,
                commands::list_projects,
                commands::create_project,
                commands::delete_project,
                commands::create_environment,
                commands::delete_environment,
                commands::list_variables,
                commands::all_variables,
                commands::vault_health,
                commands::reveal_variable,
                commands::copy_variable,
                commands::sync_now,
                secrets::copy_secret,
                devices::create_enrollment_code,
                devices::copy_enrollment_code,
                commands::set_variable,
                commands::delete_variable,
                commands::hide_variable,
                commands::restore_variable,
                commands::set_environment_parent,
                commands::clear_import_files,
                commands::preview_import,
                commands::import_variables,
                commands::get_settings,
                storage::set_backup_dir,
                storage::back_up_now,
                storage::move_vault_to_folder,
                storage::webdav_has_vault,
                secrets::list_secrets,
                secrets::get_secret,
                secrets::reveal_secret,
                secrets::generate_secret_value,
                secrets::share_preview,
                secrets::share_variable,
                secrets::link_variable,
                secrets::unlink_variable,
                secrets::stage_secret,
                secrets::promote_secret,
                secrets::roll_back_secret,
                secrets::retire_previous_secret,
                secrets::delete_secret,
                devices::list_devices,
                broker::agent_activity,
                integrations::list_services,
                integrations::list_connections,
                integrations::add_connection,
                integrations::remove_connection,
                integrations::replace_connection_token,
                integrations::secret_integrations,
                integrations::set_secret_provider,
                integrations::add_secret_target,
                integrations::remove_secret_target,
                integrations::open_provider_console,
                integrations::list_proxy_profiles,
                integrations::add_proxy_profile,
                integrations::remove_proxy_profile,
                integrations::rotation_queue,
                integrations::dismiss_rotation_queue,
            ])
            .build(mock_context(noop_assets()))
            .expect("mock app builds");
        let webview = WebviewWindowBuilder::new(&app, "main", Default::default()).build().expect("mock window");
        Desktop { webview, state, _app: app }
    }

    /// A window connected to a new vault in `dir`, unlocked.
    fn with_vault(dir: &Path) -> Desktop {
        Vault::create(FolderStore::new(dir), PASSWORD, FAST).unwrap();
        let desktop = Desktop::new();
        desktop.ok("connect_vault", json!({ "path": dir, "password": PASSWORD }));
        desktop
    }

    fn call(&self, cmd: &str, args: Value) -> Result<Value, Value> {
        let request = InvokeRequest {
            cmd: cmd.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            // The app's own origin; commands are refused from anywhere else.
            url: "tauri://localhost".parse().unwrap(),
            body: InvokeBody::Json(args),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.into(),
        };
        get_ipc_response(&self.webview, request).map(|body| body.deserialize::<Value>().unwrap())
    }

    fn ok(&self, cmd: &str, args: Value) -> Value {
        self.call(cmd, args).unwrap_or_else(|e| panic!("{cmd} failed: {e}"))
    }

    /// The error's `kind`, for a call that must fail.
    fn err(&self, cmd: &str, args: Value) -> Value {
        match self.call(cmd, args) {
            Ok(v) => panic!("{cmd} should have failed, got {v}"),
            Err(e) => e,
        }
    }

    /// A project with its `development` environment's id.
    fn project(&self, name: &str) -> (String, String) {
        let p = self.ok("create_project", json!({ "name": name }));
        (p["id"].as_str().unwrap().into(), p["environments"][0]["id"].as_str().unwrap().into())
    }

    fn set(&self, env: &str, key: &str, value: &str) {
        self.ok("set_variable", json!({ "environmentId": env, "key": key, "value": value }));
    }

    fn keys(&self, env: &str) -> Vec<String> {
        let vars = self.ok("list_variables", json!({ "environmentId": env }));
        vars.as_array().unwrap().iter().map(|v| v["key"].as_str().unwrap().to_string()).collect()
    }
}

#[test]
fn creating_a_vault_unlocks_it_and_returns_a_recovery_key() {
    let dir = tempfile::tempdir().unwrap();
    let desktop = Desktop::new();
    let status = desktop.ok("status", json!({}));
    assert_eq!((status["configured"].clone(), status["unlocked"].clone()), (json!(false), json!(false)));
    assert_eq!(desktop.ok("folder_has_vault", json!({ "path": dir.path() })), json!(false));

    let short = desktop.err("create_vault", json!({ "path": dir.path(), "password": "short" }));
    assert_eq!(short["kind"], "other");

    let recovery = desktop.ok("create_vault", json!({ "path": dir.path(), "password": PASSWORD }));
    assert_eq!(recovery.as_str().unwrap().split('-').count(), 13);
    assert_eq!(desktop.ok("folder_has_vault", json!({ "path": dir.path() })), json!(true));
    let status = desktop.ok("status", json!({}));
    assert_eq!((status["configured"].clone(), status["unlocked"].clone()), (json!(true), json!(true)));
}

#[test]
fn locked_vaults_refuse_everything_until_the_right_password() {
    let dir = tempfile::tempdir().unwrap();
    let desktop = Desktop::with_vault(dir.path());
    desktop.ok("lock", json!({}));

    for (cmd, args) in [
        ("list_projects", json!({})),
        ("all_variables", json!({})),
        ("list_secrets", json!({})),
        ("create_project", json!({ "name": "api" })),
    ] {
        assert_eq!(desktop.err(cmd, args)["kind"], "locked", "{cmd} while locked");
    }
    let wrong = desktop.err("unlock", json!({ "password": "not the password" }));
    assert_eq!((wrong["kind"].as_str(), wrong["message"].as_str()), (Some("invalid_password"), Some("Wrong password.")));
    desktop.ok("unlock", json!({ "password": PASSWORD }));
    assert_eq!(desktop.ok("list_projects", json!({})), json!([]));
}

#[test]
fn projects_environments_and_variables() {
    let dir = tempfile::tempdir().unwrap();
    let desktop = Desktop::with_vault(dir.path());
    let (project, dev) = desktop.project("api");
    assert_eq!(desktop.err("create_project", json!({ "name": "API" }))["message"], "A project named API already exists.");

    desktop.set(&dev, "DATABASE_URL", "postgres://localhost/app");
    desktop.set(&dev, "API_KEY", "sk_test_123");
    assert_eq!(desktop.keys(&dev), ["API_KEY", "DATABASE_URL"]);
    let reveal = |key: &str| desktop.ok("reveal_variable", json!({ "environmentId": dev, "key": key }));
    assert_eq!(reveal("API_KEY"), "sk_test_123");

    // Adding over an existing name is refused; editing in place and renaming work.
    let taken = desktop.err("set_variable", json!({ "environmentId": dev, "key": "API_KEY", "value": "x" }));
    assert_eq!(taken["message"], "API_KEY already exists.");
    let bad = desktop.err("set_variable", json!({ "environmentId": dev, "key": "1BAD", "value": "x" }));
    assert!(bad["message"].as_str().unwrap().contains("letters, numbers and underscores"));
    desktop.ok(
        "set_variable",
        json!({ "environmentId": dev, "key": "API_KEY", "value": "sk_test_456", "previousKey": "API_KEY" }),
    );
    desktop.ok(
        "set_variable",
        json!({ "environmentId": dev, "key": "STRIPE_KEY", "value": "sk_test_456", "previousKey": "API_KEY" }),
    );
    assert_eq!(desktop.keys(&dev), ["DATABASE_URL", "STRIPE_KEY"]);
    desktop.ok("delete_variable", json!({ "environmentId": dev, "key": "DATABASE_URL" }));
    assert_eq!(desktop.keys(&dev), ["STRIPE_KEY"]);

    // Environments come back in deploy order, with their variable counts.
    let prod = desktop.ok("create_environment", json!({ "projectId": project, "name": "production" }));
    desktop.ok("create_environment", json!({ "projectId": project, "name": "staging" }));
    let listed = desktop.ok("list_projects", json!({}));
    let envs: Vec<(&str, u64)> = listed[0]["environments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| (e["name"].as_str().unwrap(), e["varCount"].as_u64().unwrap()))
        .collect();
    assert_eq!(envs, [("development", 1), ("staging", 0), ("production", 0)]);

    desktop.ok("delete_environment", json!({ "environmentId": prod["id"] }));
    assert_eq!(desktop.ok("list_projects", json!({}))[0]["environments"].as_array().unwrap().len(), 2);
    desktop.ok("delete_project", json!({ "projectId": project }));
    assert_eq!(desktop.ok("list_projects", json!({})), json!([]));
    assert_eq!(desktop.err("list_variables", json!({ "environmentId": dev }))["message"], "That environment no longer exists.");
}

#[test]
fn listing_commands_never_return_values() {
    let dir = tempfile::tempdir().unwrap();
    let desktop = Desktop::with_vault(dir.path());
    let (_, dev) = desktop.project("api");
    let (_, web) = desktop.project("web");
    const PLAIN: &str = "plain-value-must-not-leak-1";
    const SHARED: &str = "shared-value-must-not-leak-2";
    desktop.set(&dev, "PLAIN", PLAIN);
    desktop.set(&dev, "SHARED", SHARED);
    desktop.set(&web, "SHARED", SHARED);
    let secret = desktop.ok(
        "share_variable",
        json!({ "environmentId": dev, "key": "SHARED", "name": "shared", "link": [web] }),
    );
    let id = secret["id"].clone();
    desktop.ok("stage_secret", json!({ "id": id, "value": "staged-value-must-not-leak-3", "note": "next" }));
    desktop.ok(
        "add_connection",
        json!({ "serviceId": "vercel", "name": "Web", "settings": {}, "token": "token-must-not-leak-5" }),
    );
    let web_conn = desktop.ok("list_connections", json!({}))[0]["id"].clone();
    desktop.ok(
        "add_secret_target",
        json!({ "secretId": id, "connectionId": web_conn, "name": "SHARED", "settings": { "project": "web" } }),
    );

    let responses = [
        desktop.ok("list_projects", json!({})),
        desktop.ok("list_variables", json!({ "environmentId": dev })),
        desktop.ok("list_variables", json!({ "environmentId": web })),
        desktop.ok("all_variables", json!({})),
        desktop.ok("vault_health", json!({})),
        desktop.ok("list_secrets", json!({})),
        desktop.ok("get_secret", json!({ "id": id })),
        desktop.ok("share_preview", json!({ "environmentId": dev, "key": "PLAIN" })),
        desktop.ok("preview_import", json!({ "environmentId": dev, "source": { "kind": "text", "text": "PLAIN=import-must-not-leak-4\n" } })),
        desktop.ok("get_settings", json!({})),
        desktop.ok("status", json!({})),
        desktop.ok("list_devices", json!({})),
        desktop.ok("agent_activity", json!({})),
        desktop.ok("list_connections", json!({})),
        desktop.ok("secret_integrations", json!({ "secretId": id })),
    ];
    for response in responses {
        let text = response.to_string();
        assert!(!text.contains("must-not-leak"), "a value leaked: {text}");
    }
    // Only the reveal commands return values.
    assert_eq!(desktop.ok("reveal_variable", json!({ "environmentId": web, "key": "SHARED" })), SHARED);
    assert_eq!(desktop.ok("reveal_secret", json!({ "id": id })), SHARED);
}

#[test]
fn quick_copy_lists_every_variable_by_name() {
    let dir = tempfile::tempdir().unwrap();
    let desktop = Desktop::with_vault(dir.path());
    let (api, api_dev) = desktop.project("api");
    let (_, web_dev) = desktop.project("web");
    desktop.set(&web_dev, "B", "1");
    desktop.set(&api_dev, "A", "2");
    desktop.set(&api_dev, "LINKED", "3");
    desktop.ok("share_variable", json!({ "environmentId": api_dev, "key": "LINKED", "name": "linked", "link": [] }));

    let hits = desktop.ok("all_variables", json!({}));
    let rows: Vec<String> = hits
        .as_array()
        .unwrap()
        .iter()
        .map(|h| format!("{}/{}/{} linked={}", h["project"], h["environment"], h["key"], h["linked"]).replace('"', ""))
        .collect();
    assert_eq!(rows, ["api/development/A linked=false", "api/development/LINKED linked=true", "web/development/B linked=false"]);
    assert_eq!(hits[0]["projectId"], json!(api));
    assert_eq!(hits[0]["environmentId"], json!(api_dev));
    assert_eq!(desktop.ok("vault_health", json!({})), json!({ "stale": 0, "unreadable": 0 }));
}

#[test]
fn importing_pasted_env_text() {
    let dir = tempfile::tempdir().unwrap();
    let desktop = Desktop::with_vault(dir.path());
    let (_, dev) = desktop.project("api");
    desktop.set(&dev, "KEEP", "old");
    desktop.set(&dev, "SAME", "same");
    let source = json!({ "kind": "text", "text": "KEEP=new\nSAME=same\nADDED=1\nnot a line\n" });

    let preview = desktop.ok("preview_import", json!({ "environmentId": dev, "source": source }));
    let changes: Vec<String> = preview["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| format!("{}:{}", e["key"].as_str().unwrap(), e["change"].as_str().unwrap()))
        .collect();
    assert!(changes.is_empty(), "a parse error stops the preview: {preview}");
    assert!(!preview["errors"].as_array().unwrap().is_empty());

    let source = json!({ "kind": "text", "text": "KEEP=new\nSAME=same\nADDED=1\n" });
    let result = desktop.ok("import_variables", json!({ "environmentId": dev, "source": source, "overwrite": false }));
    assert_eq!(result, json!({ "added": 1, "updated": 0, "unchanged": 1, "skipped": 1 }));
    assert_eq!(desktop.ok("reveal_variable", json!({ "environmentId": dev, "key": "KEEP" })), "old");
    let result = desktop.ok("import_variables", json!({ "environmentId": dev, "source": source, "overwrite": true }));
    assert_eq!(result, json!({ "added": 0, "updated": 1, "unchanged": 2, "skipped": 0 }));
    assert_eq!(desktop.ok("reveal_variable", json!({ "environmentId": dev, "key": "KEEP" })), "new");
}

#[test]
fn shared_secrets_rotate_everywhere_they_are_linked() {
    let dir = tempfile::tempdir().unwrap();
    let desktop = Desktop::with_vault(dir.path());
    let (_, api) = desktop.project("api");
    let (_, web) = desktop.project("web");
    let (_, docs) = desktop.project("docs");
    let (_, other) = desktop.project("other");
    desktop.set(&api, "STRIPE_KEY", "sk_old");
    desktop.set(&web, "STRIPE_KEY", "sk_old");
    desktop.set(&docs, "STRIPE_KEY", "sk_old");
    desktop.set(&other, "STRIPE_KEY", "sk_different");

    let preview = desktop.ok("share_preview", json!({ "environmentId": api, "key": "STRIPE_KEY" }));
    let projects = |list: &Value| list.as_array().unwrap().iter().map(|u| u["project"].as_str().unwrap().to_owned()).collect::<Vec<_>>();
    assert_eq!(projects(&preview["copies"]), ["docs", "web"], "same value");
    assert_eq!(projects(&preview["different"]), ["other"], "same name, another value");
    // Only the chosen copies are linked, and never one with another value.
    let secret = desktop.ok(
        "share_variable",
        json!({ "environmentId": api, "key": "STRIPE_KEY", "name": "Stripe", "link": [web, other] }),
    );
    assert!(desktop.ok("list_variables", json!({ "environmentId": docs }))[0]["secret"].is_null());
    assert!(desktop.ok("list_variables", json!({ "environmentId": other }))[0]["secret"].is_null());
    let id = secret["id"].clone();
    assert_eq!(secret["usedIn"].as_array().unwrap().len(), 2);
    let linked = desktop.ok("list_variables", json!({ "environmentId": web }));
    assert_eq!(linked[0]["secret"]["name"], "Stripe");
    // docs was left unticked: it's reported as its own copy, then as outdated once rotated.
    let copies = |id: &Value| desktop.ok("get_secret", json!({ "id": id }))["copies"].clone();
    assert_eq!(projects(&copies(&id)), ["docs"]);
    assert_eq!(copies(&id)[0]["outdated"], false);
    // A second secret from a same-named variable gets told the name is taken.
    assert_eq!(desktop.ok("share_preview", json!({ "environmentId": other, "key": "STRIPE_KEY" }))["nameTaken"], false);
    let edit = json!({ "environmentId": web, "key": "STRIPE_KEY", "value": "x", "previousKey": "STRIPE_KEY" });
    assert!(desktop.err("set_variable", edit)["message"].as_str().unwrap().contains("linked to a secret"));

    // Staged values aren't live until promoted; the old one stays as previous.
    // Generating shows a fresh value without saving anything.
    let generated = desktop.ok("generate_secret_value", json!({ "id": id }));
    let generated = generated.as_str().unwrap();
    assert_eq!(generated.len(), 40);
    assert!(generated.chars().all(|c| c.is_ascii_alphanumeric()));
    assert_ne!(desktop.ok("generate_secret_value", json!({ "id": id })), generated);
    assert_eq!(desktop.ok("get_secret", json!({ "id": id }))["staged"], false);
    let staged = desktop.ok("stage_secret", json!({ "id": id, "value": "sk_new", "note": "quarterly" }));
    assert_eq!(staged["staged"], true);
    assert_eq!(desktop.ok("reveal_variable", json!({ "environmentId": web, "key": "STRIPE_KEY" })), "sk_old");
    let promoted = desktop.ok("promote_secret", json!({ "id": id }));
    assert_eq!((promoted["staged"].clone(), promoted["hasPrevious"].clone()), (json!(false), json!(true)));
    assert_eq!(copies(&id)[0]["outdated"], true, "docs still has the old value");
    for env in [&api, &web] {
        assert_eq!(desktop.ok("reveal_variable", json!({ "environmentId": env, "key": "STRIPE_KEY" })), "sk_new");
    }
    desktop.ok("roll_back_secret", json!({ "id": id }));
    assert_eq!(desktop.ok("reveal_secret", json!({ "id": id })), "sk_old");
    desktop.ok("stage_secret", json!({ "id": id, "value": "sk_newer", "note": null }));
    desktop.ok("promote_secret", json!({ "id": id }));
    let retired = desktop.ok("retire_previous_secret", json!({ "id": id }));
    assert_eq!(retired["hasPrevious"], false);

    // Unlinking keeps the current value as the variable's own.
    desktop.ok("unlink_variable", json!({ "environmentId": web, "key": "STRIPE_KEY" }));
    assert_eq!(desktop.ok("list_variables", json!({ "environmentId": web }))[0]["secret"], Value::Null);
    assert_eq!(desktop.ok("reveal_variable", json!({ "environmentId": web, "key": "STRIPE_KEY" })), "sk_newer");
    desktop.ok("link_variable", json!({ "environmentId": web, "key": "OTHER_NAME", "secretId": id }));
    assert_eq!(desktop.ok("reveal_variable", json!({ "environmentId": web, "key": "OTHER_NAME" })), "sk_newer");
}

#[test]
fn recovery_key_unlock_requires_a_new_password() {
    let dir = tempfile::tempdir().unwrap();
    let mut vault = Vault::create(FolderStore::new(dir.path()), PASSWORD, FAST).unwrap();
    let recovery = vault.reset_recovery_key().unwrap().to_display_string();
    drop(vault);
    let desktop = Desktop::new();
    desktop.state.0.lock().unwrap().config = Some(Config::for_vault(dir.path().to_path_buf()));

    let typo = desktop.err("unlock_with_recovery", json!({ "recoveryKey": "nope" }));
    assert!(typo["message"].as_str().unwrap().contains("13 groups"));
    desktop.ok("unlock_with_recovery", json!({ "recoveryKey": recovery.to_lowercase() }));
    assert_eq!(desktop.ok("status", json!({}))["mustChangePassword"], true);
    // The forgotten password isn't asked for.
    desktop.ok("change_password", json!({ "currentPassword": null, "newPassword": "a brand new password" }));
    assert_eq!(desktop.ok("status", json!({}))["mustChangePassword"], false);

    // From now on, changing it needs the current one.
    let missing = desktop.err("change_password", json!({ "currentPassword": null, "newPassword": "another password" }));
    assert_eq!(missing["message"], "Enter your current password.");
    let wrong = desktop.err("change_password", json!({ "currentPassword": PASSWORD, "newPassword": "another password" }));
    assert_eq!(wrong["kind"], "invalid_password");
    desktop.ok("lock", json!({}));
    assert_eq!(desktop.err("unlock", json!({ "password": PASSWORD }))["kind"], "invalid_password");
    desktop.ok("unlock", json!({ "password": "a brand new password" }));
}

#[test]
fn a_new_recovery_key_needs_the_password_and_replaces_the_old_one() {
    let dir = tempfile::tempdir().unwrap();
    let mut vault = Vault::create(FolderStore::new(dir.path()), PASSWORD, FAST).unwrap();
    let old = vault.reset_recovery_key().unwrap().to_display_string();
    drop(vault);
    let desktop = Desktop::new();
    desktop.ok("connect_vault", json!({ "path": dir.path(), "password": PASSWORD }));

    assert_eq!(desktop.err("reset_recovery_key", json!({ "password": "wrong" }))["kind"], "invalid_password");
    let new = desktop.ok("reset_recovery_key", json!({ "password": PASSWORD }));
    assert_ne!(new, json!(old));
    desktop.ok("lock", json!({}));
    assert_eq!(desktop.err("unlock_with_recovery", json!({ "recoveryKey": old }))["kind"], "invalid_password");
    desktop.ok("unlock_with_recovery", json!({ "recoveryKey": new }));
}

#[test]
fn a_second_machine_sees_the_same_vault() {
    let dir = tempfile::tempdir().unwrap();
    let first = Desktop::with_vault(dir.path());
    let (_, dev) = first.project("api");
    first.set(&dev, "TOKEN", "abc");

    let second = Desktop::new();
    let wrong = second.err("connect_vault", json!({ "path": dir.path(), "password": "nope" }));
    assert_eq!(wrong["kind"], "invalid_password");
    second.ok("connect_vault", json!({ "path": dir.path(), "password": PASSWORD }));
    assert_eq!(second.ok("reveal_variable", json!({ "environmentId": dev, "key": "TOKEN" })), "abc");
}

#[test]
fn bucket_forms_are_checked_before_anything_is_contacted() {
    let desktop = Desktop::new();
    let bucket = |endpoint: &str, key: &str| {
        json!({ "bucket": {
            "endpoint": endpoint, "region": "", "bucket": "vaults", "prefix": "dotsafe",
            "accessKeyId": key, "secretAccessKey": key,
        }})
    };
    let insecure = desktop.err("bucket_has_vault", bucket("http://example.com", "AKIA"));
    assert_eq!(insecure["message"], "The endpoint must start with https://.");
    let no_key = desktop.err("bucket_has_vault", bucket("https://example.com", " "));
    assert_eq!(no_key["message"], "Enter the access key ID and secret.");
}

#[test]
fn connections_providers_and_targets() {
    let dir = tempfile::tempdir().unwrap();
    let desktop = Desktop::with_vault(dir.path());
    let (_, dev) = desktop.project("api");
    desktop.set(&dev, "OPENAI_API_KEY", "sk-1");
    let secret = desktop.ok(
        "share_variable",
        json!({ "environmentId": dev, "key": "OPENAI_API_KEY", "name": "OpenAI", "link": [] }),
    );
    let id = secret["id"].clone();

    let services = desktop.ok("list_services", json!({}));
    let ids: Vec<&str> = services.as_array().unwrap().iter().map(|s| s["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["openai", "cloudflare_token", "anthropic", "stripe", "aws_iam", "vercel", "cloudflare_workers"]);

    // Settings are checked; the token is required.
    let add = |service: &str, name: &str, settings: Value, token: &str| {
        desktop.call("add_connection", json!({ "serviceId": service, "name": name, "settings": settings, "token": token }))
    };
    assert_eq!(add("openai", "Acme", json!({}), "t").unwrap_err()["message"], "Project ID is required.");
    assert_eq!(add("openai", "Acme", json!({ "project_id": "p" }), " ").unwrap_err()["message"], "Paste the admin token.");
    add("openai", "Acme", json!({ "project_id": "proj_1" }), "admin").unwrap();
    add("vercel", "Web", json!({}), "vercel").unwrap();
    assert_eq!(add("vercel", "web", json!({}), "x").unwrap_err()["message"], "A connection named web already exists.");
    let conns = desktop.ok("list_connections", json!({}));
    let (acme, web) = (conns[0]["id"].clone(), conns[1]["id"].clone());
    assert_eq!((conns[0]["kind"].as_str(), conns[1]["kind"].as_str()), (Some("provider"), Some("target")));

    // Providers and targets must be the right kind.
    let wrong = desktop.err("set_secret_provider", json!({ "secretId": id, "connectionId": web, "settings": {} }));
    assert_eq!(wrong["message"], "Web is a deploy target, not a provider.");
    desktop.ok("set_secret_provider", json!({ "secretId": id, "connectionId": acme, "settings": {} }));
    let target = json!({ "secretId": id, "connectionId": web, "name": "OPENAI_API_KEY", "settings": { "project": "web" } });
    desktop.ok("add_secret_target", target);
    let info = desktop.ok("secret_integrations", json!({ "secretId": id }));
    assert_eq!(info["provider"]["connectionName"], "Acme");
    assert_eq!(info["provider"]["overlap"], true);
    assert_eq!(info["targets"][0]["name"], "OPENAI_API_KEY");
    assert_eq!((info["targets"][0]["pushedAt"].clone(), info["targets"][0]["upToDate"].clone()), (Value::Null, json!(false)));
    assert_eq!(desktop.ok("list_connections", json!({}))[1]["usedBy"], 1);

    // In-use connections stay until nothing uses them.
    assert!(desktop.err("remove_connection", json!({ "id": web }))["message"].as_str().unwrap().contains("still use Web"));
    let target_id = info["targets"][0]["id"].clone();
    desktop.ok("remove_secret_target", json!({ "secretId": id, "targetId": target_id }));
    desktop.ok("remove_connection", json!({ "id": web }));
    desktop.ok("set_secret_provider", json!({ "secretId": id, "connectionId": null, "settings": {} }));
    assert_eq!(desktop.ok("secret_integrations", json!({ "secretId": id })), json!({ "provider": null, "targets": [] }));
    let names: Vec<String> = desktop.ok("list_secrets", json!({})).as_array().unwrap().iter().map(|s| s["name"].as_str().unwrap().to_string()).collect();
    assert_eq!(names, ["OpenAI"], "connection tokens stay out of the secrets list");

    // A token can be replaced; the old one is kept, retired.
    let blank = desktop.err("replace_connection_token", json!({ "id": acme, "token": " " }));
    assert_eq!(blank["message"], "Paste the new admin token.");
    desktop.ok("replace_connection_token", json!({ "id": acme, "token": "admin-2" }));
    let records = desktop.state.0.lock().unwrap().vault.clone().unwrap().list().unwrap();
    let token = records
        .iter()
        .find_map(|r| match r {
            dotsafe_core::Record::Secret(s) if s.name == "Acme admin token" => Some(s.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(token.current_value(), Some("admin-2"));
    assert!(token.versions().iter().any(|(_, v)| v.state == dotsafe_core::VersionState::Retired));
}

#[test]
fn the_rotation_queue_shrinks_as_values_are_rotated() {
    let dir = tempfile::tempdir().unwrap();
    let desktop = Desktop::with_vault(dir.path());
    let (_, dev) = desktop.project("api");
    desktop.set(&dev, "DB_URL", "postgres://old");
    desktop.set(&dev, "SHARED", "s1");
    desktop.ok("share_variable", json!({ "environmentId": dev, "key": "SHARED", "name": "shared", "link": [] }));
    assert_eq!(desktop.ok("rotation_queue", json!({})), Value::Null);

    std::thread::sleep(std::time::Duration::from_millis(20));
    let since = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64;
    if let Some(config) = desktop.state.0.lock().unwrap().config.as_mut() {
        config.rotation_queue = Some(dotsafe_core::RotationQueue { since, device: "Lost laptop".into() });
    }
    std::thread::sleep(std::time::Duration::from_millis(20));

    let queue = desktop.ok("rotation_queue", json!({}));
    assert_eq!(queue["device"], "Lost laptop");
    assert_eq!(queue["secrets"][0]["name"], "shared");
    assert_eq!(queue["secrets"][0]["hasProvider"], false);
    assert_eq!(queue["variables"][0]["key"], "DB_URL");

    let edit = json!({ "environmentId": dev, "key": "DB_URL", "value": "postgres://new", "previousKey": "DB_URL" });
    desktop.ok("set_variable", edit);
    assert_eq!(desktop.ok("rotation_queue", json!({}))["variables"], json!([]));
    desktop.ok("dismiss_rotation_queue", json!({}));
    assert_eq!(desktop.ok("rotation_queue", json!({})), Value::Null);
}

#[test]
fn copying_puts_the_value_on_the_clipboard_and_remembers_it() {
    let dir = tempfile::tempdir().unwrap();
    let desktop = Desktop::with_vault(dir.path());
    let (_, dev) = desktop.project("api");
    desktop.set(&dev, "API_KEY", "sk_copy_me");
    desktop.set(&dev, "SHARED", "shared-value");
    let secret = desktop.ok("share_variable", json!({ "environmentId": dev, "key": "SHARED", "name": "shared", "link": [] }));
    let copied = || desktop.state.0.lock().unwrap().clipboard.map(|(hash, _)| hash);

    assert_eq!(copied(), None);
    desktop.ok("copy_variable", json!({ "environmentId": dev, "key": "API_KEY" }));
    assert_eq!(copied(), Some(crate::state::hash_text("sk_copy_me")));
    // A linked variable copies its secret's current value.
    desktop.ok("copy_variable", json!({ "environmentId": dev, "key": "SHARED" }));
    assert_eq!(copied(), Some(crate::state::hash_text("shared-value")));
    desktop.ok("copy_secret", json!({ "id": secret["id"] }));
    assert_eq!(copied(), Some(crate::state::hash_text("shared-value")));
    let missing = desktop.err("copy_variable", json!({ "environmentId": dev, "key": "NOPE" }));
    assert_eq!(missing["message"], "NOPE isn't set.");

    // Enrollment codes copy too, but only one made on this Mac.
    assert!(desktop.err("copy_enrollment_code", json!({}))["message"].as_str().unwrap().contains("Add a device again"));
    let code = desktop.ok("create_enrollment_code", json!({}));
    desktop.ok("copy_enrollment_code", json!({}));
    assert_eq!(copied(), Some(crate::state::hash_text(code["code"].as_str().unwrap())));

    desktop.ok("lock", json!({}));
    assert_eq!(desktop.err("copy_variable", json!({ "environmentId": dev, "key": "API_KEY" }))["kind"], "locked");
    assert!(desktop.err("copy_enrollment_code", json!({}))["message"].as_str().unwrap().contains("Add a device again"), "codes are forgotten on lock");
}

#[test]
fn syncing_a_folder_vault_is_a_no_op() {
    let dir = tempfile::tempdir().unwrap();
    let desktop = Desktop::with_vault(dir.path());
    assert_eq!(desktop.ok("sync_now", json!({})), json!({ "lastSyncedAt": null, "error": null }));
}

#[test]
fn guided_and_aws_providers_are_set_up_like_the_others() {
    let dir = tempfile::tempdir().unwrap();
    let desktop = Desktop::with_vault(dir.path());
    let (_, dev) = desktop.project("api");
    desktop.set(&dev, "STRIPE_KEY", "sk_live_1");
    desktop.set(&dev, "AWS_SECRET_ACCESS_KEY", "secret-1");
    desktop.set(&dev, "AWS_ACCESS_KEY_ID", "AKIA1");
    let share = |key: &str| {
        desktop.ok("share_variable", json!({ "environmentId": dev, "key": key, "name": key, "link": [] }))["id"].clone()
    };
    let (stripe_secret, aws_secret) = (share("STRIPE_KEY"), share("AWS_SECRET_ACCESS_KEY"));
    share("AWS_ACCESS_KEY_ID");

    // Stripe needs no token, so none is stored.
    let add = |service: &str, name: &str, settings: Value, token: &str| {
        desktop.ok("add_connection", json!({ "serviceId": service, "name": name, "settings": settings, "token": token }))
    };
    add("stripe", "Stripe", json!({}), "");
    add("aws_iam", "AWS", json!({ "access_key_id": "AKIAADMIN" }), "admin-secret");
    let conns = desktop.ok("list_connections", json!({}));
    let id_of = |name: &str| conns.as_array().unwrap().iter().find(|c| c["name"] == name).unwrap()["id"].clone();
    let secrets: Vec<String> = desktop.ok("list_secrets", json!({})).as_array().unwrap().iter().map(|s| s["name"].as_str().unwrap().to_string()).collect();
    assert_eq!(secrets.len(), 3, "no token stored for Stripe, and AWS's is hidden: {secrets:?}");

    desktop.ok("set_secret_provider", json!({ "secretId": stripe_secret, "connectionId": id_of("Stripe"), "settings": {} }));
    let info = desktop.ok("secret_integrations", json!({ "secretId": stripe_secret }));
    assert_eq!((info["provider"]["guided"].clone(), info["provider"]["manualRevoke"].clone()), (json!(true), json!(true)));

    // AWS's key-ID setting names a shared secret; it's stored as that secret's id.
    let missing = desktop.err(
        "set_secret_provider",
        json!({ "secretId": aws_secret, "connectionId": id_of("AWS"), "settings": { "user_name": "ci", "access_key_id_secret": "NOPE" } }),
    );
    assert_eq!(missing["message"], "There's no single shared secret named NOPE.");
    desktop.ok(
        "set_secret_provider",
        json!({ "secretId": aws_secret, "connectionId": id_of("AWS"), "settings": { "user_name": "ci", "access_key_id_secret": "AWS_ACCESS_KEY_ID" } }),
    );
    let info = desktop.ok("secret_integrations", json!({ "secretId": aws_secret }));
    let stored = info["provider"]["settings"]["access_key_id_secret"].as_str().unwrap();
    assert!(uuid::Uuid::parse_str(stored).is_ok(), "stored as an id: {stored}");
    assert_eq!(info["provider"]["guided"], false);

    desktop.ok("open_provider_console", json!({ "serviceId": "stripe" }));
    assert!(desktop.err("open_provider_console", json!({ "serviceId": "vercel" }))["message"].as_str().unwrap().contains("no console link"));
}

#[test]
fn custom_proxy_profiles_are_checked_and_stored_in_the_vault() {
    let dir = tempfile::tempdir().unwrap();
    let desktop = Desktop::with_vault(dir.path());
    let profile = |upstream: &str| {
        json!({ "profile": {
            "name": "Mistral", "keyVar": "MISTRAL_API_KEY", "baseUrlVar": "MISTRAL_BASE_URL",
            "upstream": upstream, "basePath": "/v1", "header": "authorization", "bearer": true,
        }})
    };
    assert!(desktop.err("add_proxy_profile", profile("http://evil.example"))["message"].as_str().unwrap().contains("https://"));
    desktop.ok("add_proxy_profile", profile("https://api.mistral.ai"));
    assert_eq!(desktop.err("add_proxy_profile", profile("https://api.mistral.ai"))["message"], "MISTRAL_API_KEY already has a profile.");

    let list = desktop.ok("list_proxy_profiles", json!({}));
    let names: Vec<&str> = list.as_array().unwrap().iter().map(|p| p["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["OpenAI", "Anthropic", "Mistral"]);
    assert_eq!(list[0]["id"], Value::Null, "built-ins can't be removed");
    let id = list[2]["id"].clone();
    desktop.ok("remove_proxy_profile", json!({ "id": id }));
    assert_eq!(desktop.ok("list_proxy_profiles", json!({})).as_array().unwrap().len(), 2);
}

#[test]
fn inherited_variables_show_where_they_come_from() {
    let dir = tempfile::tempdir().unwrap();
    let desktop = Desktop::with_vault(dir.path());
    let (project, production) = desktop.project("api");
    let staging = desktop.ok("create_environment", json!({ "projectId": project, "name": "staging" }))["id"].as_str().unwrap().to_string();
    desktop.set(&production, "DB_HOST", "db.prod");
    desktop.set(&production, "DB_URL", "postgres://${{ DB_HOST }}/app");
    desktop.set(&production, "SENTRY", "https://s");
    desktop.set(&staging, "DB_HOST", "db.staging");

    desktop.ok("set_environment_parent", json!({ "environmentId": staging, "parent": production }));
    let looped = desktop.err("set_environment_parent", json!({ "environmentId": production, "parent": staging }));
    assert!(looped["message"].as_str().unwrap().contains("loop"), "{looped}");
    let projects = desktop.ok("list_projects", json!({}));
    let env = projects[0]["environments"].as_array().unwrap().iter().find(|e| e["id"] == json!(staging)).unwrap().clone();
    assert_eq!(env["parent"], json!(production));

    desktop.ok("hide_variable", json!({ "environmentId": staging, "key": "SENTRY" }));
    let rows = desktop.ok("list_variables", json!({ "environmentId": staging }));
    let row = |key: &str| rows.as_array().unwrap().iter().find(|r| r["key"] == key).unwrap().clone();
    assert_eq!(row("DB_HOST")["overrides"], "development");
    assert_eq!((row("DB_URL")["inheritedFrom"].clone(), row("DB_URL")["references"].clone()), (json!("development"), json!(true)));
    assert_eq!(row("SENTRY")["removed"], true);
    // Reveal and quick copy see the final values and names.
    assert_eq!(desktop.ok("reveal_variable", json!({ "environmentId": staging, "key": "DB_URL" })), "postgres://db.staging/app");
    // Editing gets the value as written, so references survive.
    let raw = desktop.ok("reveal_variable", json!({ "environmentId": staging, "key": "DB_URL", "raw": true }));
    assert_eq!(raw, "postgres://${{ DB_HOST }}/app");
    assert_eq!(desktop.err("reveal_variable", json!({ "environmentId": staging, "key": "SENTRY" }))["message"], "SENTRY isn't set.");
    let hits = desktop.ok("all_variables", json!({}));
    assert!(hits.as_array().unwrap().iter().any(|h| h["environmentId"] == json!(staging) && h["key"] == "DB_URL"));

    desktop.ok("restore_variable", json!({ "environmentId": staging, "key": "SENTRY" }));
    assert_eq!(desktop.ok("reveal_variable", json!({ "environmentId": staging, "key": "SENTRY" })), "https://s");
}

#[test]
fn backups_and_moving_the_vault_from_the_app() {
    let dir = tempfile::tempdir().unwrap();
    let desktop = Desktop::with_vault(dir.path());
    let (_, dev) = desktop.project("api");
    desktop.set(&dev, "API_KEY", "sk_app");

    assert_eq!(desktop.err("back_up_now", json!({}))["message"], "Choose a backup folder first.");
    let backups = tempfile::tempdir().unwrap();
    desktop.ok("set_backup_dir", json!({ "path": backups.path() }));
    let at = desktop.ok("back_up_now", json!({}));
    let settings = desktop.ok("get_settings", json!({}));
    assert_eq!(settings["lastBackup"], at);
    assert_eq!(settings["storage"], "folder");
    assert_eq!(dotsafe_core::backup::snapshots(backups.path()).unwrap().len(), 1);

    let taken = desktop.err("move_vault_to_folder", json!({ "path": dir.path() }));
    assert_eq!(taken["message"], "That folder already holds a vault. Pick an empty one.");
    let target = tempfile::tempdir().unwrap();
    let new_home = target.path().join("vault");
    let old = desktop.ok("move_vault_to_folder", json!({ "path": new_home }));
    assert_eq!(old, json!(dir.path().display().to_string()));
    assert_eq!(desktop.ok("get_settings", json!({}))["vaultPath"], json!(new_home.display().to_string()));
    assert_eq!(desktop.ok("reveal_variable", json!({ "environmentId": dev, "key": "API_KEY" })), "sk_app");

    let bad = json!({ "webdav": { "url": "http://cloud.example.com/dav/", "username": "me", "password": "x" } });
    assert!(desktop.err("webdav_has_vault", bad)["message"].as_str().unwrap().contains("https://"));
}
