// Typed wrappers around the Rust commands in src-tauri/src/commands.rs.
import { invoke } from "@tauri-apps/api/core";

export type Status = {
  configured: boolean;
  unlocked: boolean;
  vaultPath: string | null;
  defaultVaultPath: string | null;
};

export type EnvironmentView = { id: string; name: string; varCount: number };
export type ProjectView = { id: string; name: string; environments: EnvironmentView[] };

export type ImportSource = { kind: "text"; text: string } | { kind: "files" };
export type ImportChange = "new" | "update" | "unchanged";
export type ImportPreview = {
  entries: { key: string; change: ImportChange; file: string | null }[];
  errors: string[];
};
export type ImportResult = { added: number; updated: number; unchanged: number; skipped: number };

export type Settings = {
  vaultPath: string | null;
  licenseKey: string | null;
  version: string;
  autoLockMinutes: number;
  clipboardClearSeconds: number;
};

export type ErrorKind = "locked" | "invalid_password" | "other";

export class ApiError extends Error {
  constructor(
    public kind: ErrorKind,
    message: string,
  ) {
    super(message);
  }
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (e) {
    if (e && typeof e === "object" && "kind" in e && "message" in e) {
      const err = new ApiError(e.kind as ErrorKind, String(e.message));
      if (err.kind === "locked") window.dispatchEvent(new Event("dotsafe:locked"));
      throw err;
    }
    throw new ApiError("other", String(e));
  }
}

export const api = {
  status: () => call<Status>("status"),
  folderHasVault: (path: string) => call<boolean>("folder_has_vault", { path }),
  createVault: (path: string, password: string) => call<string>("create_vault", { path, password }),
  connectVault: (path: string, password: string) => call<void>("connect_vault", { path, password }),
  unlock: (password: string) => call<void>("unlock", { password }),
  lock: () => call<void>("lock"),

  listProjects: () => call<ProjectView[]>("list_projects"),
  createProject: (name: string) => call<ProjectView>("create_project", { name }),
  deleteProject: (projectId: string) => call<void>("delete_project", { projectId }),
  createEnvironment: (projectId: string, name: string) =>
    call<EnvironmentView>("create_environment", { projectId, name }),
  deleteEnvironment: (environmentId: string) => call<void>("delete_environment", { environmentId }),

  listVariables: (environmentId: string) => call<string[]>("list_variables", { environmentId }),
  revealVariable: (environmentId: string, key: string) =>
    call<string>("reveal_variable", { environmentId, key }),
  copyVariable: (environmentId: string, key: string) => call<void>("copy_variable", { environmentId, key }),
  setVariable: (environmentId: string, key: string, value: string, previousKey?: string) =>
    call<void>("set_variable", { environmentId, key, value, previousKey: previousKey ?? null }),
  deleteVariable: (environmentId: string, key: string) =>
    call<void>("delete_variable", { environmentId, key }),

  /** Opens the native file picker; Rust keeps the paths, we get file names. */
  pickEnvFiles: () => call<string[]>("pick_env_files"),
  clearImportFiles: () => call<void>("clear_import_files"),
  previewImport: (environmentId: string, source: ImportSource) =>
    call<ImportPreview>("preview_import", { environmentId, source }),
  importVariables: (environmentId: string, source: ImportSource, overwrite: boolean) =>
    call<ImportResult>("import_variables", { environmentId, source, overwrite }),

  getSettings: () => call<Settings>("get_settings"),
  activateLicense: (key: string) => call<string>("activate_license", { key }),
  removeLicense: () => call<void>("remove_license"),
};

export function message(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}
