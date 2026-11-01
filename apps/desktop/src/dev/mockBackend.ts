// In-memory stand-in for the Rust commands, so the UI can be developed in a
// plain browser (`bun run dev`, then open http://localhost:1420). Only loaded
// in dev mode outside Tauri; never part of a production build.
import { emit } from "@tauri-apps/api/event";
import { mockIPC } from "@tauri-apps/api/mocks";

type Env = { id: string; projectId: string; name: string; vars: Map<string, string>; parent?: string | null; hidden?: Set<string> };

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
// `?syncoffline` makes the bucket unreachable (use with `?bucket`).
const syncOffline = new URLSearchParams(location.search).has("syncoffline");
const OFFLINE_SYNC = "Offline. Changes are saved on this Mac and sync when you're back.";
// `?update` acts as if a newer version was released.
const mockUpdate = new URLSearchParams(location.search).has("update");
const MOCK_UPDATE = {
  version: "0.2.0",
  currentVersion: "0.1.0",
  notes: "- Offline indicator for bucket vaults\n- Service logos when connecting accounts\n- Fixes from the test pass",
};
// Backups.
let backupDir: string | null = null;
let lastBackup: number | null = null;
let autoLockMinutes = 15;
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
  staging: { DATABASE_URL: "postgres://localhost:5432/app" },
  production: { DATABASE_URL: "postgres://prod.example.com/app" },
});
addProject("marketing-site", { development: {} });

// Shared secrets. A linked variable is stored in `vars` as LINK + secret id.
const LINK = "\u0000secret:";
const DAY = 24 * 60 * 60 * 1000;
type MockVersion = { id: string; value: string | null; state: string; createdAt: number; changedAt: number; note: string | null };
type MockSecret = { id: string; name: string; rotateEveryDays: number | null; versions: MockVersion[] };
const secrets = new Map<string, MockSecret>();
function addSecret(name: string, value: string, ageDays: number, rotateEveryDays: number | null = null) {
  const sid = id();
  const at = Date.now() - ageDays * DAY;
  secrets.set(sid, {
    id: sid,
    name,
    rotateEveryDays,
    versions: [{ id: id(), value, state: "current", createdAt: at, changedAt: at, note: null }],
  });
  return sid;
}
{
  // Seed: one secret linked into both environments of my-api, overdue for rotation.
  const sid = addSecret("STRIPE_SECRET_KEY", "sk_test_51Nx8abc", 120, 90);
  for (const e of envs.values()) if (projects.get(e.projectId) === "my-api") e.vars.set("STRIPE_SECRET_KEY", LINK + sid);
}
// Integrations. `?integrations` starts with an OpenAI provider and a Vercel
// target connected.
const SERVICES = [
  {
    id: "openai",
    label: "OpenAI",
    kind: "provider",
    tokenHelp: "An admin key from platform.openai.com → Organization → Admin keys.",
    connectionFields: [{ key: "project_id", label: "Project ID", hint: "proj_…, from the project's settings.", required: true, secretRef: false }],
    bindingFields: [],
    overlap: true,
    guided: false, manualRevoke: false, needsToken: true, consoleUrl: "https://platform.openai.com/api-keys",
  },
  {
    id: "cloudflare_token",
    label: "Cloudflare API token",
    kind: "provider",
    tokenHelp: 'A Cloudflare API token with the "API Tokens: Edit" permission.',
    connectionFields: [{ key: "account_id", label: "Account ID", hint: "Only for account-owned tokens.", required: false, secretRef: false }],
    bindingFields: [{ key: "token_id", label: "Token ID", hint: "The ID of the token this secret holds.", required: true, secretRef: false }],
    overlap: false,
    guided: false, manualRevoke: false, needsToken: true, consoleUrl: "https://dash.cloudflare.com/profile/api-tokens",
  },
  {
    id: "anthropic",
    label: "Anthropic",
    kind: "provider",
    tokenHelp: "An Admin key (sk-ant-admin…). dotsafe uses it to switch old keys off.",
    connectionFields: [],
    bindingFields: [],
    overlap: true,
    guided: true, manualRevoke: false, needsToken: true, consoleUrl: "https://console.anthropic.com/settings/keys",
  },
  {
    id: "stripe",
    label: "Stripe",
    kind: "provider",
    tokenHelp: "No token needed: Stripe has no API for managing keys. dotsafe checks each new key with itself.",
    connectionFields: [],
    bindingFields: [],
    overlap: true,
    guided: true, manualRevoke: true, needsToken: false, consoleUrl: "https://dashboard.stripe.com/apikeys",
  },
  {
    id: "aws_iam",
    label: "AWS IAM access key",
    kind: "provider",
    tokenHelp: "The secret access key of an IAM user allowed to create and delete access keys.",
    connectionFields: [{ key: "access_key_id", label: "Admin access key ID", hint: "AKIA…", required: true, secretRef: false }],
    bindingFields: [
      { key: "user_name", label: "IAM user", hint: "The user whose access key this secret holds.", required: true, secretRef: false },
      { key: "access_key_id_secret", label: "Secret for the key ID", hint: "Optional: the secret that holds AWS_ACCESS_KEY_ID.", required: false, secretRef: true },
    ],
    overlap: true,
    guided: false, manualRevoke: false, needsToken: true, consoleUrl: "https://console.aws.amazon.com/iam/home#/users",
  },
  {
    id: "vercel",
    label: "Vercel",
    kind: "target",
    tokenHelp: "A Vercel access token from vercel.com/account/tokens.",
    connectionFields: [{ key: "team_id", label: "Team ID", hint: "team_…, for team projects.", required: false, secretRef: false }],
    bindingFields: [
      { key: "project", label: "Project", hint: "The project's name or ID.", required: true, secretRef: false },
      { key: "environments", label: "Environments", hint: "Comma-separated. Defaults to all three.", required: false, secretRef: false },
    ],
    overlap: false,
    guided: false, manualRevoke: false, needsToken: true, consoleUrl: null,
  },
  {
    id: "cloudflare_workers",
    label: "Cloudflare Workers",
    kind: "target",
    tokenHelp: 'A Cloudflare API token with the "Workers Scripts: Edit" permission.',
    connectionFields: [{ key: "account_id", label: "Account ID", hint: "From the dashboard's sidebar.", required: true, secretRef: false }],
    bindingFields: [{ key: "script", label: "Worker", hint: "The Worker's name.", required: true, secretRef: false }],
    overlap: false,
    guided: false, manualRevoke: false, needsToken: true, consoleUrl: null,
  },
];
type MockConnection = { id: string; service: string; name: string; settings: Record<string, string>; credential: string | null };
type MockTarget = { id: string; connectionId: string; name: string; settings: Record<string, string>; pushedValue: string | null; pushedAt: number | null };
const connections = new Map<string, MockConnection>();
const bindings = new Map<string, { provider: { connectionId: string; settings: Record<string, string> } | null; targets: MockTarget[] }>();
function bindingOf(secretId: string) {
  if (!bindings.has(secretId)) bindings.set(secretId, { provider: null, targets: [] });
  return bindings.get(secretId)!;
}
function addConnection(service: string, name: string, settings: Record<string, string>) {
  const needsToken = SERVICES.find((x) => x.id === service)?.needsToken ?? true;
  const credential = needsToken ? addSecret(`${name} admin token`, "mock-admin-token", 0) : null;
  const c = { id: id(), service, name, settings, credential };
  connections.set(c.id, c);
  return c;
}
if (new URLSearchParams(location.search).has("integrations")) {
  addConnection("openai", "Acme", { project_id: "proj_123" });
  addConnection("vercel", "Acme on Vercel", { team_id: "team_1" });
}
function pushAll(secretId: string) {
  const s = secret(secretId);
  const value = currentOf(s)?.value ?? null;
  return bindingOf(secretId).targets.map((t) => {
    const c = connections.get(t.connectionId);
    if (!c) return { name: t.name, connection: "", error: "That connection was removed." };
    t.pushedValue = value;
    t.pushedAt = Date.now();
    return { name: t.name, connection: c.name, error: null };
  });
}

// Custom proxy profiles.
let proxyProfiles: { id: string; name: string; keyVar: string; baseUrlVar: string; upstream: string; problem: null }[] = [];

// The rotation queue after removing a device: values as they were then.
// `?queue` starts with one, as if "Old laptop" was removed a moment ago.
let rotationQueue: { since: number; device: string; vars: Map<string, string> } | null = null;
function startQueue(device: string) {
  const vars = new Map<string, string>();
  for (const e of envs.values()) for (const [k, v] of e.vars) if (!linkOf(v)) vars.set(`${e.id}/${k}`, v);
  rotationQueue = { since: Date.now(), device, vars };
}
function queueView() {
  if (!rotationQueue) return null;
  const q = rotationQueue;
  const tokens = new Set([...connections.values()].map((c) => c.credential));
  return {
    since: q.since,
    device: q.device,
    secrets: [...secrets.values()]
      .filter((s) => (currentOf(s)?.createdAt ?? Infinity) < q.since)
      .map((s) => {
        const p = bindings.get(s.id)?.provider;
        const svc = SERVICES.find((x) => x.id === (p && connections.get(p.connectionId)?.service));
        return { id: s.id, name: s.name, hasProvider: !!svc && !svc.guided, connectionToken: tokens.has(s.id) };
      })
      .sort((x, y) => Number(y.hasProvider) - Number(x.hasProvider) || x.name.localeCompare(y.name)),
    variables: [...envs.values()].flatMap((e) =>
      [...e.vars]
        .filter(([k, v]) => q.vars.get(`${e.id}/${k}`) === v)
        .map(([key]) => ({ projectId: e.projectId, project: projects.get(e.projectId)!, environmentId: e.id, environment: e.name, key })),
    ),
  };
}

// `?integrity` makes the integrity checks report problems.
const integrity = new URLSearchParams(location.search).has("integrity");
// Devices. `?touchid` starts as a Mac set up for Touch ID unlock.
let touchIdUnlock = new URLSearchParams(location.search).has("touchid");
const mockDevices = [
  { deviceId: "this-mac", name: "Your MacBook Pro", added: Date.now() - 40 * DAY, stale: false, thisDevice: true },
  { deviceId: "studio", name: "Studio Mac", added: Date.now() - 12 * DAY, stale: false, thisDevice: false },
];

// Agents. `?agent` sends an approval request a moment after unlocking.
type MockGrant = { command: string; cwd: string; variables: number; expiresIn: number | null };
const agentGrants: MockGrant[] = [];
// The mock's stand-in for Touch ID always confirms, after a moment.
let agentApprovalAuth = true;
const agentLog: { at: number; actor: string; action: string; outcome: string; summary: string; environment: string; variables: string[]; detail: string | null }[] = [];
const pendingAgent = new Map<string, { command: string[]; cwd: string; variables: number }>();
function fakeAgentRequest() {
  const req = {
    id: id(),
    kind: "run",
    client: "Claude Code",
    command: ["bun", "scripts/seed.ts"],
    programPath: "/opt/homebrew/bin/bun",
    cwd: "/Users/you/code/my-api",
    project: "my-api",
    environment: "development",
    variables: ["DATABASE_URL", "JWT_SECRET", "OPENAI_API_KEY", "STRIPE_SECRET_KEY"],
    proxied: ["OPENAI_API_KEY"],
    // Written by the agent a moment ago.
    files: [{ path: "/Users/you/code/my-api/scripts/seed.ts", modifiedMs: Date.now() - 20_000 }],
    generated: null,
  };
  pendingAgent.set(req.id, { command: req.command, cwd: req.cwd, variables: req.variables.length });
  emit("agent-request", req);
  // Then one asking to add a generated secret.
  const gen = {
    ...req,
    id: id(),
    kind: "generate",
    command: [],
    programPath: "",
    cwd: "",
    variables: ["SESSION_SECRET"],
    proxied: [],
    files: [],
    generated: "48 random characters",
  };
  pendingAgent.set(gen.id, { command: ["SESSION_SECRET"], cwd: "", variables: 1 });
  setTimeout(() => emit("agent-request", gen), 300);
}

const linkOf = (v: string | undefined) => (v?.startsWith(LINK) ? v.slice(LINK.length) : null);
const secret = (sid: string) => secrets.get(sid) ?? fail("other", "That secret no longer exists.");
const currentOf = (s: MockSecret) =>
  s.versions.filter((v) => v.state === "current").sort((x, y) => y.changedAt - x.changedAt)[0] ?? null;
const resolve = (v: string | undefined) => {
  const sid = linkOf(v);
  return sid ? (secrets.get(sid) && currentOf(secrets.get(sid)!)?.value) ?? undefined : v;
};
/** An environment's variables with inheritance applied: key → [raw, env it comes from]. */
function effective(e: Env): Map<string, [string, Env]> {
  const chain: Env[] = [];
  for (let cur: Env | undefined = e; cur && !chain.includes(cur); cur = cur.parent ? envs.get(cur.parent) : undefined) chain.push(cur);
  const out = new Map<string, [string, Env]>();
  for (const layer of chain.reverse()) {
    for (const k of layer.hidden ?? []) out.delete(k);
    for (const [k, v] of layer.vars) out.set(k, [v, layer]);
  }
  return out;
}
/** The final value: links resolved, `${{ NAME }}` expanded (one level is plenty for the mock). */
function finalValue(e: Env, key: string, depth = 0): string | undefined {
  const raw = effective(e).get(key)?.[0];
  const value = resolve(raw);
  if (value === undefined || depth > 8) return value;
  return value.replace(/(?<!\$)\$\{\{\s*([A-Za-z_][A-Za-z0-9_]*)\s*\}\}/g, (_, name: string) => {
    const inner = finalValue(e, name, depth + 1);
    if (inner === undefined) fail("other", `${key} refers to ${name}, which isn't set`);
    return inner!;
  });
}
function secretView(s: MockSecret) {
  const current = currentOf(s);
  const age = current ? Date.now() - current.createdAt : 0;
  const limit = (s.rotateEveryDays ?? 0) * DAY;
  return {
    id: s.id,
    name: s.name,
    rotatedAt: current?.createdAt ?? null,
    due: !s.rotateEveryDays ? "no_policy" : age >= limit ? "overdue" : age + 7 * DAY >= limit ? "soon" : "ok",
    rotateEveryDays: s.rotateEveryDays,
    staged: s.versions.some((v) => v.state === "pending"),
    hasPrevious: s.versions.some((v) => v.state === "previous"),
    usedIn: [...envs.values()].flatMap((e) =>
      [...e.vars].filter(([, v]) => linkOf(v) === s.id).map(([key]) => ({
        projectId: e.projectId,
        project: projects.get(e.projectId) ?? "",
        environmentId: e.id,
        environment: e.name,
        key,
      })),
    ),
  };
}
const secretDetail = (s: MockSecret) => ({
  ...secretView(s),
  versions: [...s.versions]
    .sort((x, y) => y.createdAt - x.createdAt)
    .map(({ value, ...v }) => ({ ...v, wiped: value === null })),
  copies: unlinkedCopies(s),
});
/** Variables with their own copy of one of `s`'s values, instead of a link. */
function unlinkedCopies(s: MockSecret) {
  const current = currentOf(s)?.value;
  const values = s.versions.filter((v) => v.state !== "pending" && v.value !== null).map((v) => v.value);
  return [...envs.values()].flatMap((e) =>
    [...e.vars]
      .filter(([, v]) => values.includes(v))
      .map(([key, v]) => ({ projectId: e.projectId, project: projects.get(e.projectId), environmentId: e.id, environment: e.name, key, outdated: v !== current })),
  );
}
function promote(s: MockSecret, now: number) {
  for (const v of s.versions) {
    const next = v.state === "pending" ? "current" : v.state === "current" ? "previous" : v.state === "previous" ? "retired" : null;
    if (next) Object.assign(v, { state: next, changedAt: now });
  }
}
const newValue = (value: string | null | undefined) =>
  value ?? Array.from(crypto.getRandomValues(new Uint8Array(24)), (b) => b.toString(36).padStart(2, "0")).join("").slice(0, 40);

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
  if (new URLSearchParams(location.search).has("queue")) startQueue("Old laptop");
  mockIPC(
    async (cmd, raw) => {
      const a = (raw ?? {}) as Record<string, string>;
      // Integration commands also take settings maps.
      const settings = ((raw ?? {}) as { settings?: Record<string, string> }).settings ?? {};
      await new Promise((r) => setTimeout(r, 120));
      const needsUnlock = ![
        "status",
        "folder_has_vault",
        "create_vault",
        "connect_vault",
        "bucket_has_vault",
        "unlock_with_touch_id",
        "connect_with_code",
        "create_bucket_vault",
        "connect_bucket_vault",
        "unlock",
        "unlock_with_recovery",
        "lock",
      ].includes(cmd);
      if (needsUnlock && !unlocked) fail("locked", "The vault is locked.");

      switch (cmd) {
        case "status":
          return {
            configured,
            unlocked,
            mustChangePassword,
            vaultPath: storage.path,
            defaultVaultPath: "/Users/you/Library/Mobile Documents/com~apple~CloudDocs/dotsafe",
            touchIdUnlock,
            passwordDue: false,
            canJoinWithCode: true,
          };
        case "unlock_with_touch_id":
          await delay(700);
          if (!touchIdUnlock) fail("other", "Use your password to unlock.");
          unlocked = true;
          return null;
        case "set_touch_id_unlock":
          await delay(500);
          touchIdUnlock = (raw as { enabled: boolean }).enabled;
          return null;
        case "list_devices":
          return {
            devices: mockDevices.filter((d) => !d.thisDevice || touchIdUnlock),
            touchIdUnlock,
            touchIdAvailable: true,
          };
        case "copy_enrollment_code":
          return null;
        case "create_enrollment_code":
          await delay(600);
          return { code: "7QXP-4HTV-W3NB-8RJD-6YFC-2GAS-5Z", expiresAt: Date.now() + 10 * 60_000 };
        case "revoke_device": {
          await delay(800);
          if ((raw as { password: string }).password !== password) fail("invalid_password", "Wrong password.");
          const i = mockDevices.findIndex((d) => d.deviceId === (raw as { deviceId: string }).deviceId);
          if (i >= 0) {
            startQueue(mockDevices[i].name);
            mockDevices.splice(i, 1);
          }
          return recoveryKey;
        }
        case "connect_with_code": {
          await delay(600);
          const code = ((raw as { code: string }).code ?? "").replace(/[^0-9a-z]/gi, "");
          if (code.length !== 26) fail("other", "That isn't a code. It's 7 groups of letters and numbers.");
          configured = unlocked = touchIdUnlock = true;
          return null;
        }
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
          if (new URLSearchParams(location.search).has("agent")) setTimeout(fakeAgentRequest, 1500);
          if (mockUpdate) setTimeout(() => emit("update-available", MOCK_UPDATE), 1500);
          return null;
        case "answer_agent_request": {
          if ((raw as { lifetime: string | null }).lifetime && agentApprovalAuth) await delay(700);
          const req = pendingAgent.get(a.id) ?? fail("other", "That request already expired.");
          pendingAgent.delete(a.id);
          const lifetime = (raw as { lifetime: string | null }).lifetime;
          const command = req.command.join(" ");
          if (lifetime && lifetime !== "once")
            agentGrants.push({ command, cwd: req.cwd, variables: req.variables, expiresIn: lifetime === "lock" ? null : lifetime === "1h" ? 3600 : 900 });
          const generate = req.cwd === "";
          agentLog.unshift({
            at: Date.now(),
            actor: "Claude Code",
            action: generate ? "generate" : "run",
            outcome: lifetime ? "allowed" : "denied",
            summary: command,
            environment: "my-api/development",
            variables: [],
            detail: lifetime ? (generate ? "48 random characters" : "exit 0") : null,
          });
          emit("agent-activity", null);
          return null;
        }
        case "agent_activity":
          return { approvalAuth: agentApprovalAuth, approvalAuthAvailable: true, grants: agentGrants, recent: agentLog };
        case "set_agent_approval_auth":
          await delay(400);
          agentApprovalAuth = (raw as { required: boolean }).required;
          return null;
        case "revoke_agent_grants":
          agentGrants.length = 0;
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
                .map((e) => ({ id: e.id, name: e.name, varCount: e.vars.size, parent: e.parent ?? null })),
            }))
            .sort((x, y) => x.name.localeCompare(y.name));
        case "create_project": {
          if ([...projects.values()].some((n) => n.toLowerCase() === a.name.toLowerCase()))
            fail("other", `A project named ${a.name} already exists.`);
          const pid = addProject(a.name, { development: {} });
          const e = [...envs.values()].find((x) => x.projectId === pid)!;
          return { id: pid, name: a.name, environments: [{ id: e.id, name: e.name, varCount: 0, parent: null }] };
        }
        case "delete_project":
          projects.delete(a.projectId);
          for (const [eid, e] of envs) if (e.projectId === a.projectId) envs.delete(eid);
          return null;
        case "create_environment": {
          const eid = id();
          envs.set(eid, { id: eid, projectId: a.projectId, name: a.name, vars: new Map() });
          return { id: eid, name: a.name, varCount: 0, parent: null };
        }
        case "delete_environment":
          envs.delete(a.environmentId);
          return null;
        case "list_variables": {
          const e = env(a.environmentId);
          const parent = e.parent ? envs.get(e.parent) : undefined;
          const fromParent = parent ? effective(parent) : new Map<string, [string, Env]>();
          const rows = [...effective(e)].map(([key, [v, layer]]) => {
            const sid = linkOf(v);
            const own = layer.id === e.id;
            return {
              key,
              secret: sid ? { id: sid, name: secrets.get(sid)?.name ?? "Deleted secret" } : null,
              references: !sid && v.includes("${{"),
              inheritedFrom: own ? null : layer.name,
              overrides: own ? (fromParent.get(key)?.[1].name ?? null) : null,
              removed: false,
            };
          });
          for (const key of e.hidden ?? [])
            if (fromParent.has(key))
              rows.push({ key, secret: null, references: false, inheritedFrom: fromParent.get(key)![1].name, overrides: null, removed: true });
          return rows.sort((x, y) => x.key.localeCompare(y.key));
        }
        case "hide_variable": {
          const e = env(a.environmentId);
          e.vars.delete(a.key);
          (e.hidden ??= new Set()).add(a.key);
          return null;
        }
        case "restore_variable":
          env(a.environmentId).hidden?.delete(a.key);
          return null;
        case "set_environment_parent": {
          const e = env(a.environmentId);
          for (let cur = a.parent ? envs.get(a.parent) : undefined; cur; cur = cur.parent ? envs.get(cur.parent) : undefined)
            if (cur.id === e.id) fail("other", `${e.name} is part of a loop of environments inheriting from each other`);
          e.parent = a.parent || null;
          return null;
        }
        case "all_variables":
          return [...envs.values()]
            .flatMap((e) =>
              [...e.vars].map(([key, v]) => ({
                projectId: e.projectId,
                project: projects.get(e.projectId)!,
                environmentId: e.id,
                environment: e.name,
                key,
                linked: !!linkOf(v),
              })),
            )
            .sort((x, y) => x.project.localeCompare(y.project) || x.environment.localeCompare(y.environment) || x.key.localeCompare(y.key));
        case "list_services":
          return SERVICES;
        case "list_connections":
          return [...connections.values()]
            .map((c) => {
              const svc = SERVICES.find((x) => x.id === c.service)!;
              const usedBy = [...bindings.values()].filter(
                (b) => b.provider?.connectionId === c.id || b.targets.some((t) => t.connectionId === c.id),
              ).length;
              return { id: c.id, service: c.service, serviceLabel: svc.label, kind: svc.kind, name: c.name, settings: c.settings, usedBy };
            })
            .sort((x, y) => x.name.localeCompare(y.name));
        case "add_connection": {
          const svc = SERVICES.find((x) => x.id === a.serviceId) ?? fail("other", `dotsafe doesn't know ${a.serviceId}`);
          for (const f of svc.connectionFields) if (f.required && !settings[f.key]?.trim()) fail("other", `${f.label} is required.`);
          if (!a.token.trim() && svc.needsToken) fail("other", "Paste the admin token.");
          if ([...connections.values()].some((c) => c.name.toLowerCase() === a.name.trim().toLowerCase()))
            fail("other", `A connection named ${a.name.trim()} already exists.`);
          await delay(300);
          addConnection(svc.id, a.name.trim(), settings);
          return null;
        }
        case "replace_connection_token": {
          const c = connections.get(a.id) ?? fail("other", "That connection was removed.");
          if (!a.token.trim()) fail("other", "Paste the new admin token.");
          if (!c.credential) fail("other", `${c.name} doesn't use a token.`);
          const s = secret(c.credential!);
          const now = Date.now();
          s.versions.push({ id: id(), value: a.token.trim(), state: "pending", createdAt: now, changedAt: now, note: "Replaced" });
          promote(s, now);
          for (const v of s.versions) if (v.state === "previous") Object.assign(v, { state: "retired", changedAt: now });
          return null;
        }
        case "remove_connection": {
          const c = connections.get(a.id) ?? fail("other", "That connection was already removed.");
          const used = [...bindings.values()].some((b) => b.provider?.connectionId === c.id || b.targets.some((t) => t.connectionId === c.id));
          if (used) fail("other", `Secrets still use ${c.name}. Remove it from them first.`);
          connections.delete(c.id);
          if (c.credential) secrets.delete(c.credential);
          return null;
        }
        case "secret_integrations": {
          const b = bindingOf(a.secretId);
          const current = currentOf(secret(a.secretId))?.value ?? null;
          const label = (c?: MockConnection) => SERVICES.find((x) => x.id === c?.service)?.label ?? "Unknown";
          const pc = b.provider ? connections.get(b.provider.connectionId) : undefined;
          return {
            provider: b.provider && pc
              ? (() => {
                  const svc = SERVICES.find((x) => x.id === pc.service)!;
                  return {
                    connectionId: pc.id, connectionName: pc.name, serviceId: pc.service, serviceLabel: label(pc),
                    settings: b.provider.settings, overlap: svc.overlap, guided: svc.guided, manualRevoke: svc.manualRevoke,
                  };
                })()
              : null,
            targets: b.targets.map((t) => {
              const c = connections.get(t.connectionId);
              return {
                id: t.id, connectionId: t.connectionId, connectionName: c?.name ?? "Removed connection", serviceLabel: label(c),
                name: t.name, settings: t.settings, pushedAt: t.pushedAt, upToDate: t.pushedAt !== null && t.pushedValue === current,
              };
            }),
          };
        }
        case "set_secret_provider": {
          const b = bindingOf(a.secretId);
          if (a.connectionId) {
            const c = connections.get(a.connectionId) ?? fail("other", "That connection was removed.");
            if (SERVICES.find((x) => x.id === c.service)!.kind !== "provider") fail("other", `${c.name} is a deploy target, not a provider.`);
          }
          b.provider = a.connectionId ? { connectionId: a.connectionId, settings: settings } : null;
          return null;
        }
        case "add_secret_target": {
          const c = connections.get(a.connectionId) ?? fail("other", "That connection was removed.");
          if (SERVICES.find((x) => x.id === c.service)!.kind !== "target") fail("other", `${c.name} is a provider, not a deploy target.`);
          bindingOf(a.secretId).targets.push({ id: id(), connectionId: c.id, name: a.name, settings: settings, pushedValue: null, pushedAt: null });
          return null;
        }
        case "remove_secret_target": {
          const b = bindingOf(a.secretId);
          b.targets = b.targets.filter((t) => t.id !== a.targetId);
          return null;
        }
        case "rotate_with_provider": {
          const s = secret(a.secretId);
          const b = bindingOf(a.secretId);
          const c = (b.provider && connections.get(b.provider.connectionId)) ?? fail("other", `${s.name} has no provider.`);
          if (s.versions.some((v) => v.state === "pending")) fail("other", `${s.name} already has a staged value; promote or cancel it first.`);
          const svc = SERVICES.find((x) => x.id === c.service)!;
          if (svc.guided && !a.pasted?.trim()) fail("other", `Paste the new key you made in ${svc.label}.`);
          await delay(900);
          const now = Date.now();
          const value = svc.guided ? a.pasted.trim() : `sk-mock-${now}`;
          s.versions.push({ id: id(), value, state: "pending", createdAt: now, changedAt: now, note: svc.guided ? `Made in ${svc.label}` : `Issued by ${c.name}` });
          promote(s, now);
          const revokedOld = (Boolean(a.leaked) || !svc.overlap) && !svc.manualRevoke;
          if (revokedOld) for (const v of s.versions) if (v.state === "previous") Object.assign(v, { state: "retired", changedAt: now });
          return { checkFailed: null, promoted: true, revokedOld, pushes: pushAll(s.id) };
        }
        case "list_proxy_profiles":
          return [
            { id: null, name: "OpenAI", keyVar: "OPENAI_API_KEY", baseUrlVar: "OPENAI_BASE_URL", upstream: "https://api.openai.com", problem: null },
            { id: null, name: "Anthropic", keyVar: "ANTHROPIC_API_KEY", baseUrlVar: "ANTHROPIC_BASE_URL", upstream: "https://api.anthropic.com", problem: null },
            ...proxyProfiles,
          ];
        case "add_proxy_profile": {
          const p = (raw as { profile: { name: string; keyVar: string; baseUrlVar: string; upstream: string } }).profile;
          if (!p.upstream.startsWith("https://")) fail("other", "The upstream must start with https://.");
          if (["OPENAI_API_KEY", "ANTHROPIC_API_KEY"].includes(p.keyVar)) fail("other", `${p.keyVar} already goes through the proxy.`);
          proxyProfiles.push({ id: id(), name: p.name, keyVar: p.keyVar, baseUrlVar: p.baseUrlVar, upstream: p.upstream, problem: null });
          return null;
        }
        case "remove_proxy_profile":
          proxyProfiles = proxyProfiles.filter((p) => p.id !== a.id);
          return null;
        case "open_provider_console":
          return null;
        case "push_secret":
          await delay(500);
          return pushAll(a.secretId);
        case "revoke_previous_with_provider": {
          const s = secret(a.secretId);
          const b = bindingOf(a.secretId);
          const svc = SERVICES.find((x) => x.id === (b.provider && connections.get(b.provider.connectionId)?.service));
          if (svc?.manualRevoke && !a.byHand) fail("other", `${svc.label} keys can only be revoked in its dashboard.`);
          await delay(500);
          let n = 0;
          for (const v of s.versions) if (v.state === "previous") (Object.assign(v, { state: "retired", changedAt: Date.now() }), n++);
          return n;
        }
        case "rotation_queue":
          return queueView();
        case "rotate_queued_with_providers": {
          const view = queueView();
          const out = [];
          for (const q of view?.secrets.filter((s) => s.hasProvider) ?? []) {
            await delay(400);
            const s = secret(q.id);
            const now = Date.now();
            s.versions.push({ id: id(), value: `sk-mock-${now}`, state: "pending", createdAt: now, changedAt: now, note: "Issued by provider" });
            promote(s, now);
            out.push({ name: s.name, error: null, pushes: pushAll(s.id) });
          }
          return out;
        }
        case "dismiss_rotation_queue":
          rotationQueue = null;
          return null;
        case "vault_health":
          return integrity ? { stale: 1, unreadable: 2, damaged: 1 } : { stale: 0, unreadable: 0, damaged: 0 };
        case "reveal_variable": {
          const e = env(a.environmentId);
          if (!effective(e).has(a.key)) fail("other", `${a.key} isn't set.`);
          const asWritten = (raw as { raw?: boolean } | undefined)?.raw;
          return asWritten ? resolve(effective(e).get(a.key)![0]) : finalValue(e, a.key);
        }
        case "list_secrets": {
          const tokens = new Set([...connections.values()].map((c) => c.credential));
          return [...secrets.values()].filter((x) => !tokens.has(x.id)).map(secretView).sort((x, y) => x.name.localeCompare(y.name));
        }
        case "get_secret":
          return secretDetail(secret(a.id));
        case "reveal_secret":
          return currentOf(secret(a.id))?.value ?? fail("other", "This secret has no value.");
        case "copy_secret":
          return null;
        case "generate_secret_value":
          return Array.from(crypto.getRandomValues(new Uint8Array(40)), (b) => "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789"[b % 62]).join("");
        case "share_preview": {
          const value = env(a.environmentId).vars.get(a.key);
          const usage = (e: Env) => ({ projectId: e.projectId, project: projects.get(e.projectId), environmentId: e.id, environment: e.name, key: a.key });
          const others = [...envs.values()].filter((e) => e.id !== a.environmentId && e.vars.has(a.key) && !e.vars.get(a.key)!.startsWith(LINK));
          return {
            copies: others.filter((e) => e.vars.get(a.key) === value).map(usage),
            different: others.filter((e) => e.vars.get(a.key) !== value).map(usage),
            nameTaken: [...secrets.values()].some((s) => s.name === a.key),
          };
        }
        case "share_variable": {
          const e = env(a.environmentId);
          const value = e.vars.get(a.key) ?? fail("other", `${a.key} isn't set.`);
          const sid = addSecret((raw as { name: string }).name, value, 0);
          for (const other of envs.values())
            if ((raw as { link: string[] }).link.includes(other.id) && other.vars.get(a.key) === value) other.vars.set(a.key, LINK + sid);
          e.vars.set(a.key, LINK + sid);
          return secretView(secret(sid));
        }
        case "link_variable":
          env(a.environmentId).vars.set(a.key, LINK + (raw as { secretId: string }).secretId);
          return null;
        case "unlink_variable": {
          const e = env(a.environmentId);
          e.vars.set(a.key, resolve(e.vars.get(a.key)) ?? "");
          return null;
        }
        case "stage_secret": {
          const s = secret(a.id);
          if (s.versions.some((v) => v.state === "pending")) fail("other", "a new value is already staged; promote or cancel it first");
          const now = Date.now();
          s.versions.push({ id: id(), value: newValue(a.value), state: "pending", createdAt: now, changedAt: now, note: a.note ?? null });
          if ((raw as { live: boolean }).live) promote(s, now);
          return secretDetail(s);
        }
        case "promote_secret": {
          const s = secret(a.id);
          promote(s, Date.now());
          return secretDetail(s);
        }
        case "cancel_staged_secret": {
          const s = secret(a.id);
          for (const v of s.versions) if (v.state === "pending") Object.assign(v, { state: "retired", value: null, changedAt: Date.now() });
          return secretDetail(s);
        }
        case "roll_back_secret": {
          const s = secret(a.id);
          const previous = s.versions.find((v) => v.state === "previous") ?? fail("other", "there's no previous value to go back to");
          for (const v of s.versions) if (v.state === "current") Object.assign(v, { state: "retired", changedAt: Date.now() });
          Object.assign(previous, { state: "current", changedAt: Date.now() });
          return secretDetail(s);
        }
        case "retire_previous_secret": {
          const s = secret(a.id);
          for (const v of s.versions) if (v.state === "previous") Object.assign(v, { state: "retired", changedAt: Date.now() });
          return secretDetail(s);
        }
        case "rotate_leaked_secret": {
          const s = secret(a.id);
          const now = Date.now();
          for (const v of s.versions) if (v.state === "pending") Object.assign(v, { state: "retired", value: null, changedAt: now });
          s.versions.push({ id: id(), value: newValue(a.value), state: "pending", createdAt: now, changedAt: now, note: "Leaked" });
          promote(s, now);
          for (const v of s.versions) if (v.state === "previous") Object.assign(v, { state: "retired", changedAt: now });
          return secretDetail(s);
        }
        case "restore_secret_version": {
          const s = secret(a.id);
          const old = s.versions.find((v) => v.id === a.versionId) ?? fail("other", "that version doesn't exist");
          const now = Date.now();
          s.versions.push({ id: id(), value: old.value, state: "pending", createdAt: now, changedAt: now, note: "Restored an earlier value" });
          promote(s, now);
          return secretDetail(s);
        }
        case "update_secret_settings": {
          const s = secret(a.id);
          s.name = a.name.trim() || fail("other", "Give the secret a name.");
          s.rotateEveryDays = (raw as { rotateEveryDays: number | null }).rotateEveryDays;
          return secretDetail(s);
        }
        case "delete_secret": {
          const s = secret(a.id);
          const value = currentOf(s)?.value ?? "";
          for (const e of envs.values()) for (const [k, v] of e.vars) if (linkOf(v) === s.id) e.vars.set(k, value);
          secrets.delete(s.id);
          return null;
        }
        case "copy_variable":
          return null;
        case "set_variable": {
          const e = env(a.environmentId);
          if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(a.key))
            fail("other", "Use letters, numbers and underscores, not starting with a number.");
          if (a.previousKey !== a.key && e.vars.has(a.key)) fail("other", `${a.key} already exists.`);
          if (a.previousKey === a.key && linkOf(e.vars.get(a.key)))
            fail("other", `${a.key} is linked to a secret. Rotate the secret, or unlink ${a.key} first.`);
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
              change: !e.vars.has(k)
                ? "new"
                : linkOf(e.vars.get(k))
                  ? "linked"
                  : e.vars.get(k) === v
                    ? "unchanged"
                    : "update",
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
            else if (linkOf(e.vars.get(k))) { r.skipped++; continue; }
            else if (e.vars.get(k) === v) { r.unchanged++; continue; }
            else if ((raw as { overwrite?: boolean }).overwrite) r.updated++;
            else { r.skipped++; continue; }
            e.vars.set(k, v);
          }
          importFiles = [];
          return r;
        }
        case "webdav_has_vault": {
          await delay(500);
          const w = (raw as { webdav: { url: string; username: string; password: string } }).webdav;
          if (!w.url.startsWith("https://")) fail("other", "a WebDAV address must start with https://");
          if (w.password === "wrong") fail("other", `${w.url} refused the username or password`);
          return w.url.includes("existing");
        }
        case "create_webdav_vault":
        case "connect_webdav_vault": {
          await delay(600);
          const w = (raw as { webdav: { url: string } }).webdav;
          if (cmd === "connect_webdav_vault" && a.password !== password) fail("invalid_password", "Wrong password.");
          storage = { kind: "webdav", path: w.url.replace(/\/+$/, "") };
          configured = true;
          unlocked = true;
          return cmd === "create_webdav_vault" ? recoveryKey : null;
        }
        case "set_backup_dir":
          backupDir = (raw as { path: string | null }).path;
          return null;
        case "back_up_now":
          if (!backupDir) fail("other", "Choose a backup folder first.");
          await delay(400);
          lastBackup = Date.now();
          return lastBackup;
        case "move_vault_to_folder": {
          await delay(700);
          const old = storage.path;
          storage = { kind: "folder", path: (raw as { path: string }).path };
          return old;
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
        case "check_for_update":
          await delay(500);
          return mockUpdate ? MOCK_UPDATE : null;
        case "install_update": {
          // Pretend to download 8 MB, then "restart".
          const total = 8_000_000;
          for (let downloaded = 0; downloaded <= total; downloaded += 1_000_000) {
            emit("update-progress", { downloaded, total });
            await delay(250);
          }
          location.reload();
          return null;
        }
        case "sync_now":
          await delay(600);
          if (syncOffline) fail("offline", OFFLINE_SYNC);
          lastSyncedAt = Date.now();
          return { lastSyncedAt, error: null };
        case "set_auto_lock":
          autoLockMinutes = (raw as { minutes: number }).minutes;
          return null;
        case "get_settings":
          return {
            vaultPath: storage.path,
            storage: storage.kind,
            lastSyncedAt: storage.kind !== "folder" ? lastSyncedAt : null,
            syncError: syncOffline && storage.kind !== "folder" ? OFFLINE_SYNC : null,
            backupDir,
            lastBackup,
            license,
            version: "0.1.0",
            autoLockMinutes,
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
