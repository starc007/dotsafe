// In-memory stand-in for the Rust commands, so the UI can be developed in a
// plain browser (`bun run dev`, then open http://localhost:1420). Only loaded
// in dev mode outside Tauri; never part of a production build.
import { mockIPC } from "@tauri-apps/api/mocks";

type Env = { id: string; projectId: string; name: string; vars: Map<string, string> };

const PASSWORD = "password123";
let configured = new URLSearchParams(location.search).get("setup") === null;
let unlocked = false;
// Stand-in for files picked or dropped: name -> contents.
const SAMPLE_FILES: Record<string, string> = {
  ".env": "DATABASE_URL=postgres://localhost:5432/app\nREDIS_URL=redis://localhost:6379\nSENTRY_DSN=https://abc@sentry.io/1\n",
  ".env.local": "DATABASE_URL=postgres://localhost:5432/app_local\nFEATURE_FLAGS=beta\n",
};
let importFiles: string[] = [];
// `?bucket` starts with a vault in an S3 bucket instead of iCloud Drive.
let storage = new URLSearchParams(location.search).has("bucket")
  ? { kind: "s3", path: "s3://my-vaults/dotsafe" }
  : { kind: "folder", path: "/Users/you/Library/Mobile Documents/com~apple~CloudDocs/dotsafe" };
let lastSyncedAt: number | null = Date.now() - 3 * 60_000;
// `?offline` makes every license server call fail. Keys containing LIMIT or
// REVOKED act out those answers.
const licenseOffline = new URLSearchParams(location.search).has("offline");
let license: { key: string; deviceName: string; activatedAt: number; checkedAt: number; status: string } | null = null;
let mustChangePassword = false;
let password = PASSWORD;
let recoveryKey = "7K2M-9QXP-4HTV-W3NB-8RJD-6YFC-2GAS-5ZEK-1MPQ-0VXT-3HBN-9WCR-4DQA";
const normalizeKey = (k: string) => k.toUpperCase().replace(/[\s-]/g, "").replace(/[IL]/g, "1").replace(/O/g, "0");
const projects = new Map<string, string>();
const envs = new Map<string, Env>();

const id = () => crypto.randomUUID();
const delay = (ms: number) => new Promise((r) => setTimeout(r, ms));
function addProject(name: string, data: Record<string, Record<string, string>>) {
  const pid = id();
  projects.set(pid, name);
  for (const [envName, vars] of Object.entries(data)) {
    const eid = id();
    envs.set(eid, { id: eid, projectId: pid, name: envName, vars: new Map(Object.entries(vars)) });
  }
  return pid;
}

addProject("my-api", {
  development: {
    DATABASE_URL: "postgres://localhost:5432/app",
    REDIS_URL: "redis://localhost:6379",
    STRIPE_SECRET_KEY: "sk_test_51Nx8abc",
    JWT_SECRET: "dev-secret-change-me",
  },
  production: { DATABASE_URL: "postgres://prod.example.com/app" },
});
addProject("marketing-site", { development: {} });

const fail = (kind: string, message: string) => {
  throw { kind, message };
};
const env = (eid: string) => envs.get(eid) ?? fail("other", "That environment no longer exists.");
function parseEnv(text: string): { vars: [string, string][]; errors: string[] } {
  const vars: [string, string][] = [];
  const errors: string[] = [];
  text.split("\n").forEach((raw, i) => {
    const line = raw.trim().replace(/^export\s+/, "");
    if (!line || line.startsWith("#")) return;
    const eq = line.indexOf("=");
    const key = line.slice(0, eq).trim();
    if (eq < 0 || !/^[A-Za-z_][A-Za-z0-9_]*$/.test(key)) errors.push(`Line ${i + 1}: expected KEY=value`);
    else vars.push([key, line.slice(eq + 1).trim().replace(/^["']|["']$/g, "")]);
  });
  // Like the Rust parser: one bad line rejects the whole input.
  return { vars: errors.length ? [] : vars, errors: errors.slice(0, 1) };
}
function collect(source: { kind: string; text?: string }) {
  if (source.kind === "text") return parseEnv(source.text ?? "");
  const all = importFiles.map((f) => ({ f, ...parseEnv(SAMPLE_FILES[f] ?? "") }));
  const merged = new Map<string, [string, string]>();
  all.forEach(({ f, vars }) => vars.forEach(([k, v]) => merged.set(k, [v, f])));
  return { vars: [...merged].map(([k, [v]]) => [k, v] as [string, string]), errors: [], files: merged };
}
const rank = (n: string) => ({ development: 0, staging: 1, production: 2 })[n] ?? 3;

export function installMockBackend() {
  mockIPC(
    async (cmd, raw) => {
      const a = (raw ?? {}) as Record<string, string>;
      await new Promise((r) => setTimeout(r, 120));
      const needsUnlock = ![
        "status",
        "folder_has_vault",
        "create_vault",
        "connect_vault",
        "bucket_has_vault",
        "create_bucket_vault",
        "connect_bucket_vault",
        "unlock",
        "unlock_with_recovery",
        "lock",
      ].includes(cmd);
      if (needsUnlock && !unlocked) fail("locked", "The vault is locked.");

      switch (cmd) {
        case "status":
          return { configured, unlocked, mustChangePassword, vaultPath: storage.path, defaultVaultPath: "/Users/you/Library/Mobile Documents/com~apple~CloudDocs/dotsafe" };
        case "folder_has_vault":
          return false;
        case "create_vault":
          configured = unlocked = true;
          return "7K2M-9QXP-4HTV-W3NB-8RJD-6YFC-2GAS-5ZEK-1MPQ-0VXT-3HBN-9WCR-4DQA";
        case "connect_vault":
        case "unlock":
          if (a.password !== password) fail("invalid_password", "Wrong password.");
          configured = unlocked = true;
          mustChangePassword = false;
          return null;
        case "unlock_with_recovery":
          if (normalizeKey(a.recoveryKey ?? "").length !== 52)
            fail("other", "That isn't a recovery key. It's 13 groups of 4 letters and numbers.");
          if (normalizeKey(a.recoveryKey) !== normalizeKey(recoveryKey))
            fail("invalid_password", "That recovery key doesn't match this vault.");
          unlocked = true;
          mustChangePassword = true;
          return null;
        case "change_password":
          if ((a.newPassword ?? "").length < 8) fail("other", "Use at least 8 characters.");
          if (!mustChangePassword && a.currentPassword !== password)
            fail("invalid_password", "Your current password is wrong.");
          password = a.newPassword;
          mustChangePassword = false;
          return null;
        case "reset_recovery_key":
          if (a.password !== password) fail("invalid_password", "Your current password is wrong.");
          recoveryKey = "Q8ZD-2MKT-7HWX-4NRC-9BVA-3YEJ-6PGS-1TFK-5LMQ-0XWD-8HCN-2RVB-7JQA";
          return recoveryKey;
        case "lock":
          unlocked = false;
          mustChangePassword = false;
          return null;
        case "list_projects":
          return [...projects]
            .map(([pid, name]) => ({
              id: pid,
              name,
              environments: [...envs.values()]
                .filter((e) => e.projectId === pid)
                .sort((x, y) => rank(x.name) - rank(y.name))
                .map((e) => ({ id: e.id, name: e.name, varCount: e.vars.size })),
            }))
            .sort((x, y) => x.name.localeCompare(y.name));
        case "create_project": {
          if ([...projects.values()].some((n) => n.toLowerCase() === a.name.toLowerCase()))
            fail("other", `A project named ${a.name} already exists.`);
          const pid = addProject(a.name, { development: {} });
          const e = [...envs.values()].find((x) => x.projectId === pid)!;
          return { id: pid, name: a.name, environments: [{ id: e.id, name: e.name, varCount: 0 }] };
        }
        case "delete_project":
          projects.delete(a.projectId);
          for (const [eid, e] of envs) if (e.projectId === a.projectId) envs.delete(eid);
          return null;
        case "create_environment": {
          const eid = id();
          envs.set(eid, { id: eid, projectId: a.projectId, name: a.name, vars: new Map() });
          return { id: eid, name: a.name, varCount: 0 };
        }
        case "delete_environment":
          envs.delete(a.environmentId);
          return null;
        case "list_variables":
          return [...env(a.environmentId).vars.keys()].sort();
        case "reveal_variable":
          return env(a.environmentId).vars.get(a.key) ?? fail("other", `${a.key} isn't set.`);
        case "copy_variable":
          return null;
        case "set_variable": {
          const e = env(a.environmentId);
          if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(a.key))
            fail("other", "Use letters, numbers and underscores, not starting with a number.");
          if (a.previousKey !== a.key && e.vars.has(a.key)) fail("other", `${a.key} already exists.`);
          if (a.previousKey && a.previousKey !== a.key) e.vars.delete(a.previousKey);
          e.vars.set(a.key, a.value);
          return null;
        }
        case "delete_variable":
          env(a.environmentId).vars.delete(a.key);
          return null;
        case "pick_env_files":
          importFiles = Object.keys(SAMPLE_FILES);
          return importFiles;
        case "clear_import_files":
          importFiles = [];
          return null;
        case "preview_import": {
          const e = env(a.environmentId);
          const c = collect(a.source as unknown as { kind: string; text?: string });
          const files = (c as { files?: Map<string, [string, string]> }).files;
          return {
            entries: c.vars.map(([k, v]) => ({
              key: k,
              change: !e.vars.has(k) ? "new" : e.vars.get(k) === v ? "unchanged" : "update",
              file: files?.get(k)?.[1] ?? null,
            })),
            errors: c.errors,
          };
        }
        case "import_variables": {
          const e = env(a.environmentId);
          const c = collect(a.source as unknown as { kind: string; text?: string });
          const r = { added: 0, updated: 0, unchanged: 0, skipped: 0 };
          for (const [k, v] of c.vars) {
            if (!e.vars.has(k)) r.added++;
            else if (e.vars.get(k) === v) { r.unchanged++; continue; }
            else if ((raw as { overwrite?: boolean }).overwrite) r.updated++;
            else { r.skipped++; continue; }
            e.vars.set(k, v);
          }
          importFiles = [];
          return r;
        }
        case "bucket_has_vault": {
          await delay(500);
          const b = (raw as { bucket: { endpoint: string; bucket: string } }).bucket;
          if (!b.endpoint.startsWith("https://")) fail("other", "The endpoint must start with https://.");
          if (b.bucket === "offline") fail("offline", "Couldn't reach s3://offline. Check your connection and try again.");
          return b.bucket === "existing";
        }
        case "create_bucket_vault":
        case "connect_bucket_vault": {
          await delay(500);
          const b = (raw as { bucket: { bucket: string; prefix: string }; password: string });
          if (cmd === "connect_bucket_vault" && b.password !== password) fail("invalid_password", "Wrong password.");
          configured = true;
          unlocked = true;
          storage = { kind: "s3", path: `s3://${b.bucket.bucket}${b.bucket.prefix ? `/${b.bucket.prefix}` : ""}` };
          lastSyncedAt = Date.now();
          return cmd === "create_bucket_vault" ? recoveryKey : null;
        }
        case "sync_now":
          await delay(600);
          lastSyncedAt = Date.now();
          return { lastSyncedAt, error: null };
        case "get_settings":
          return {
            vaultPath: storage.path,
            storage: storage.kind,
            lastSyncedAt: storage.kind === "s3" ? lastSyncedAt : null,
            syncError: null,
            license,
            version: "0.1.0",
            autoLockMinutes: 15,
            clipboardClearSeconds: 30,
          };
        case "activate_license": {
          const key = (a.key ?? "").trim();
          if (!key) fail("other", "Enter your license key.");
          if (/\s/.test(key) || key.length < 8)
            fail("other", "That doesn't look like a license key. Copy it again from your purchase email.");
          await delay(600);
          if (licenseOffline) fail("offline", "Couldn't reach the license server. Check your connection and try again.");
          if (/LIMIT/i.test(key))
            fail("other", "This key is already in use on the maximum number of devices. Deactivate it on another device first.");
          license = { key, deviceName: "Your MacBook Pro", activatedAt: Date.now(), checkedAt: Date.now(), status: "active" };
          return license;
        }
        case "check_license":
          await delay(500);
          if (licenseOffline) fail("offline", "Couldn't reach the license server. Check your connection and try again.");
          if (license) license = { ...license, checkedAt: Date.now(), status: /REVOKED/i.test(license.key) ? "revoked" : "active" };
          return license;
        case "remove_license":
          await delay(400);
          if (licenseOffline && !(raw as { force?: boolean }).force)
            fail("offline", "Couldn't reach the license server. Check your connection and try again.");
          license = null;
          return null;
        case "plugin:dialog|open":
          return null;
        default:
          return null;
      }
    },
    { shouldMockEvents: true },
  );
  console.info(`dotsafe mock backend: unlock with "${PASSWORD}" or recovery key ${recoveryKey}. Add ?setup to start at setup.`);
}
