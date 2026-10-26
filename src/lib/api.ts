// Typed wrappers around the Rust commands in src-tauri/src/commands.rs.
import { invoke } from "@tauri-apps/api/core";

export type Status = {
  configured: boolean;
  unlocked: boolean;
  vaultPath: string | null;
  defaultVaultPath: string | null;
  /** Unlocked with the recovery key; a new password must be set first. */
  mustChangePassword: boolean;
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

export type LicenseStatus = "active" | "unverified" | "revoked";
export type License = {
  key: string;
  deviceName: string;
  /** Milliseconds since the epoch. */
  activatedAt: number;
  /** When the license server last confirmed it. */
  checkedAt: number;
  status: LicenseStatus;
};

/** An S3-compatible bucket and its access key, as entered during setup. */
export type BucketInput = {
  endpoint: string;
  region: string;
  bucket: string;
  prefix: string;
  accessKeyId: string;
  secretAccessKey: string;
};

export type SyncView = { lastSyncedAt: number | null; error: string | null };

export type Settings = {
  /** A folder path, or `s3://bucket/prefix` for a vault in a bucket. */
  vaultPath: string | null;
  storage: "folder" | "s3";
  /** Milliseconds since the epoch. Bucket vaults only. */
  lastSyncedAt: number | null;
  syncError: string | null;
  license: License | null;
  version: string;
  autoLockMinutes: number;
  clipboardClearSeconds: number;
};

/** `offline`: the license server or the vault's bucket couldn't be reached. */
export type ErrorKind = "locked" | "invalid_password" | "offline" | "other";

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
  unlockWithRecovery: (recoveryKey: string) => call<void>("unlock_with_recovery", { recoveryKey }),
  changePassword: (currentPassword: string | null, newPassword: string) =>
    call<void>("change_password", { currentPassword, newPassword }),
  resetRecoveryKey: (password: string) => call<string>("reset_recovery_key", { password }),
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

  bucketHasVault: (bucket: BucketInput) => call<boolean>("bucket_has_vault", { bucket }),
  createBucketVault: (bucket: BucketInput, password: string) =>
    call<string>("create_bucket_vault", { bucket, password }),
  connectBucketVault: (bucket: BucketInput, password: string) =>
    call<void>("connect_bucket_vault", { bucket, password }),
  syncNow: () => call<SyncView>("sync_now"),
  getSettings: () => call<Settings>("get_settings"),
  activateLicense: (key: string) => call<License>("activate_license", { key }),
  checkLicense: () => call<License | null>("check_license"),
  /** `force` forgets the license even if the server can't be told. */
  removeLicense: (force = false) => call<void>("remove_license", { force }),
};

export function message(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}
