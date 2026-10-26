// In-memory stand-in for the Rust commands, so the UI can be developed in a
// plain browser (`bun run dev`, then open http://localhost:1420). Only loaded
// in dev mode outside Tauri; never part of a production build.
import { mockIPC } from "@tauri-apps/api/mocks";

type Env = { id: string; projectId: string; name: string; vars: Map<string, string> };

const PASSWORD = "password123";
let configured = new URLSearchParams(location.search).get("setup") === null;
let unlocked = false;
const projects = new Map<string, string>();
const envs = new Map<string, Env>();

const id = () => crypto.randomUUID();
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
const rank = (n: string) => ({ development: 0, staging: 1, production: 2 })[n] ?? 3;

export function installMockBackend() {
  mockIPC(
    async (cmd, raw) => {
      const a = (raw ?? {}) as Record<string, string>;
      await new Promise((r) => setTimeout(r, 120));
      const needsUnlock = !["status", "folder_has_vault", "create_vault", "connect_vault", "unlock", "lock"].includes(cmd);
      if (needsUnlock && !unlocked) fail("locked", "The vault is locked.");

      switch (cmd) {
        case "status":
          return { configured, unlocked, vaultPath: "/Users/you/Library/Mobile Documents/com~apple~CloudDocs/dotsafe", defaultVaultPath: "/Users/you/Library/Mobile Documents/com~apple~CloudDocs/dotsafe" };
        case "folder_has_vault":
          return false;
        case "create_vault":
          configured = unlocked = true;
          return "7K2M-9QXP-4HTV-W3NB-8RJD-6YFC-2GAS-5ZEK-1MPQ-0VXT-3HBN-9WCR-4DQA";
        case "connect_vault":
        case "unlock":
          if (a.password !== PASSWORD) fail("invalid_password", "Wrong password.");
          configured = unlocked = true;
          return null;
        case "lock":
          unlocked = false;
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
        case "plugin:dialog|open":
          return null;
        default:
          return null;
      }
    },
    { shouldMockEvents: true },
  );
  console.info(`dotsafe mock backend: unlock with "${PASSWORD}". Add ?setup to start at setup.`);
}
