// Typed wrappers around the Rust commands in src-tauri/src/commands.rs.
import { invoke } from "@tauri-apps/api/core";

export type Status = {
  /** A license is activated on this Mac. dotsafe asks for one before anything else. */
  licensed: boolean;
  configured: boolean;
  unlocked: boolean;
  vaultPath: string | null;
  defaultVaultPath: string | null;
  /** Unlocked with the recovery key; a new password must be set first. */
  mustChangePassword: boolean;
  /** Offer Touch ID on the unlock screen. */
  touchIdUnlock: boolean;
  /** Touch ID is set up, but it's time to enter the password again. */
  passwordDue: boolean;
  /** This Mac can join a vault with a code from another device. */
  canJoinWithCode: boolean;
};

export type DeviceView = { deviceId: string; name: string; added: number; stale: boolean; thisDevice: boolean };
export type DevicesView = { devices: DeviceView[]; touchIdUnlock: boolean; touchIdAvailable: boolean };

export type EnvironmentView = { id: string; name: string; varCount: number; parent: string | null };
export type ProjectView = { id: string; name: string; environments: EnvironmentView[] };

export type ImportSource = { kind: "text"; text: string } | { kind: "files" };
export type ImportChange = "new" | "update" | "unchanged" | "linked";

export type SecretRef = { id: string; name: string };
/** A variable's name, and the shared secret it links to, if any. */
/** A variable's name and where it comes from. Never its value. */
export type VariableView = {
  key: string;
  secret: SecretRef | null;
  /** Its value includes other variables with `${{ NAME }}`. */
  references: boolean;
  /** Set by an ancestor environment, not this one: its name. */
  inheritedFrom: string | null;
  /** Set here, replacing what the named ancestor has. */
  overrides: string | null;
  /** Removed here: the ancestor in `inheritedFrom` has it, but it doesn't apply. */
  removed: boolean;
};

/** A variable anywhere in the vault, for the quick-copy palette. Names only. */
export type VariableHit = {
  projectId: string;
  project: string;
  environmentId: string;
  environment: string;
  key: string;
  linked: boolean;
};
/** What the integrity checks found. */
export type VaultHealth = {
  /** Records storage served an older copy of than one already seen. */
  stale: number;
  /** Records sealed with a key the vault no longer has. */
  unreadable: number;
  /** Records that failed authentication or aren't valid. */
  damaged: number;
};

/** A setting a service needs. */
export type ServiceField = {
  key: string;
  label: string;
  hint: string;
  required: boolean;
  /** The value is the name of another shared secret. */
  secretRef: boolean;
};
/** Something dotsafe can connect to: a provider that issues keys, or a deploy target. */
export type ServiceView = {
  id: string;
  label: string;
  kind: "provider" | "target";
  tokenHelp: string;
  connectionFields: ServiceField[];
  bindingFields: ServiceField[];
  /** Providers: the old key keeps working until it's revoked. */
  overlap: boolean;
  /** Providers: you make keys in the console and paste them. */
  guided: boolean;
  /** Providers: old keys are revoked in the console, then marked retired. */
  manualRevoke: boolean;
  needsToken: boolean;
  consoleUrl: string | null;
};
/** A connected account. Its admin token never leaves Rust. */
export type ConnectionView = {
  id: string;
  service: string;
  serviceLabel: string;
  kind: "provider" | "target";
  name: string;
  settings: Record<string, string>;
  usedBy: number;
};
export type SecretIntegrations = {
  provider: {
    connectionId: string;
    connectionName: string;
    serviceId: string;
    serviceLabel: string;
    settings: Record<string, string>;
    overlap: boolean;
    /** You make new keys in the console and paste them. */
    guided: boolean;
    /** Old keys are revoked in the console. */
    manualRevoke: boolean;
  } | null;
  targets: {
    id: string;
    connectionId: string;
    connectionName: string;
    serviceLabel: string;
    name: string;
    settings: Record<string, string>;
    pushedAt: number | null;
    /** The last push matches the current value. */
    upToDate: boolean;
  }[];
};
/** What still needs rotating after a device was removed. */
export type QueueView = {
  since: number;
  device: string;
  secrets: { id: string; name: string; hasProvider: boolean; connectionToken: boolean }[];
  variables: { projectId: string; project: string; environmentId: string; environment: string; key: string }[];
};
/** A key the secret proxy keeps out of processes. `id` is null for built-ins. */
export type ProxyProfileView = {
  id: string | null;
  name: string;
  keyVar: string;
  baseUrlVar: string;
  upstream: string;
  /** Why a custom profile isn't used. */
  problem: string | null;
};
export type ProxyProfileInput = {
  name: string;
  keyVar: string;
  baseUrlVar: string;
  upstream: string;
  basePath: string;
  header: string;
  bearer: boolean;
};
export type PushView = { name: string; connection: string; error: string | null };
export type RotationView = {
  /** Why the new key failed its check. */
  checkFailed: string | null;
  /** The new key is live (false: it was left staged). */
  promoted: boolean;
  revokedOld: boolean;
  pushes: PushView[];
};

export type Due = "no_policy" | "ok" | "soon" | "overdue";
export type Usage = { projectId: string; project: string; environmentId: string; environment: string; key: string };
export type SecretView = {
  id: string;
  name: string;
  /** When the current value was created, in ms. */
  rotatedAt: number | null;
  due: Due;
  rotateEveryDays: number | null;
  /** A new value is staged but not live yet. */
  staged: boolean;
  /** An older value is still marked valid, waiting to be revoked. */
  hasPrevious: boolean;
  usedIn: Usage[];
};
export type VersionState = "pending" | "current" | "previous" | "retired";
export type VersionView = {
  id: string;
  state: VersionState;
  createdAt: number;
  changedAt: number;
  note: string | null;
  wiped: boolean;
};
/** A variable with its own copy of one of a secret's values, instead of a link. */
export type CopyView = Usage & { outdated: boolean };
export type SecretDetail = SecretView & { versions: VersionView[]; copies: CopyView[] };
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

/** A WebDAV folder (Nextcloud, ownCloud, a NAS) and its login, as entered during setup. */
export type WebDavInput = { url: string; username: string; password: string };

export type SyncView = { lastSyncedAt: number | null; error: string | null };

/** A newer version of the app, found by a check. */
export type UpdateView = { version: string; currentVersion: string; notes: string | null };
/** Download progress in bytes; `total` is null when unknown. */
export type UpdateProgress = { downloaded: number; total: number | null };

/** An agent asking to run a command with secrets, or to add a generated
 * secret (from the broker). */
export type ApprovalRequest = {
  id: string;
  kind: "run" | "generate";
  client: string;
  project: string;
  environment: string;
  /** Run: program and arguments. */
  command: string[];
  programPath: string;
  cwd: string;
  /** Run: variables it gets. Generate: the one to create. */
  variables: string[];
  /** Run: keys that go through the secret proxy instead. */
  proxied: string[];
  /** Run: scripts it runs, and when each last changed (Unix ms). */
  files: { path: string; modifiedMs: number | null }[];
  /** Generate: what kind of value. */
  generated: string | null;
};
export type GrantLifetime = "once" | "15m" | "1h" | "lock";
/** An entry in the vault's audit log. */
export type AgentEvent = {
  at: number;
  actor: string;
  /** `rotate`, `push` and `revoke` are your own rotations through integrations. */
  action: "run" | "generate" | "rotate" | "push" | "revoke";
  outcome: "allowed" | "denied" | "refused" | "failed";
  summary: string;
  environment: string;
  variables: string[];
  detail?: string | null;
};
export type AgentActivity = {
  /** Approving asks for Touch ID or the login password. */
  approvalAuth: boolean;
  /** This Mac can ask for it. */
  approvalAuthAvailable: boolean;
  grants: { command: string; cwd: string; variables: number; expiresIn: number | null }[];
  recent: AgentEvent[];
};

export type Settings = {
  /** A folder path, `s3://bucket/prefix`, or a WebDAV URL. */
  vaultPath: string | null;
  storage: "folder" | "s3" | "webdav";
  /** Where backup snapshots go, if set. */
  backupDir: string | null;
  /** When the last backup was taken (ms). */
  lastBackup: number | null;
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
  setAutoLock: (minutes: number) => call<void>("set_auto_lock", { minutes }),
  lock: () => call<void>("lock"),

  listProjects: () => call<ProjectView[]>("list_projects"),
  createProject: (name: string) => call<ProjectView>("create_project", { name }),
  deleteProject: (projectId: string) => call<void>("delete_project", { projectId }),
  createEnvironment: (projectId: string, name: string) =>
    call<EnvironmentView>("create_environment", { projectId, name }),
  deleteEnvironment: (environmentId: string) => call<void>("delete_environment", { environmentId }),

  listVariables: (environmentId: string) => call<VariableView[]>("list_variables", { environmentId }),
  allVariables: () => call<VariableHit[]>("all_variables"),
  vaultHealth: () => call<VaultHealth>("vault_health"),
  listSecrets: () => call<SecretView[]>("list_secrets"),
  getSecret: (id: string) => call<SecretDetail>("get_secret", { id }),
  revealSecret: (id: string) => call<string>("reveal_secret", { id }),
  /** A fresh random value from the secret's generator; nothing is saved. */
  generateSecretValue: (id: string) => call<string>("generate_secret_value", { id }),
  copySecret: (id: string) => call<void>("copy_secret", { id }),
  sharePreview: (environmentId: string, key: string) =>
    call<{ copies: Usage[]; different: Usage[]; nameTaken: boolean }>("share_preview", { environmentId, key }),
  /** `link`: environment ids from the preview's copies to link as well. */
  shareVariable: (environmentId: string, key: string, name: string, link: string[]) =>
    call<SecretView>("share_variable", { environmentId, key, name, link }),
  linkVariable: (environmentId: string, key: string, secretId: string) =>
    call<void>("link_variable", { environmentId, key, secretId }),
  unlinkVariable: (environmentId: string, key: string) => call<void>("unlink_variable", { environmentId, key }),
  /** `value: null` generates one. `live` skips staging: it goes live at once. */
  stageSecret: (id: string, value: string | null, note: string | null, live: boolean) =>
    call<SecretDetail>("stage_secret", { id, value, note, live }),
  promoteSecret: (id: string) => call<SecretDetail>("promote_secret", { id }),
  cancelStagedSecret: (id: string) => call<SecretDetail>("cancel_staged_secret", { id }),
  rollBackSecret: (id: string) => call<SecretDetail>("roll_back_secret", { id }),
  retirePreviousSecret: (id: string) => call<SecretDetail>("retire_previous_secret", { id }),
  rotateLeakedSecret: (id: string, value: string | null) => call<SecretDetail>("rotate_leaked_secret", { id, value }),
  restoreSecretVersion: (id: string, versionId: string) =>
    call<SecretDetail>("restore_secret_version", { id, versionId }),
  updateSecretSettings: (id: string, name: string, rotateEveryDays: number | null) =>
    call<SecretDetail>("update_secret_settings", { id, name, rotateEveryDays }),
  deleteSecret: (id: string) => call<void>("delete_secret", { id }),
  /** `raw`: the value as written, with `${{ NAME }}` references, for editing. */
  revealVariable: (environmentId: string, key: string, raw = false) =>
    call<string>("reveal_variable", { environmentId, key, raw }),
  hideVariable: (environmentId: string, key: string) => call<void>("hide_variable", { environmentId, key }),
  restoreVariable: (environmentId: string, key: string) => call<void>("restore_variable", { environmentId, key }),
  /** `parent: null` stops inheriting. */
  setEnvironmentParent: (environmentId: string, parent: string | null) =>
    call<void>("set_environment_parent", { environmentId, parent }),
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
  /** `null` when this is the latest version. */
  checkForUpdate: () => call<UpdateView | null>("check_for_update"),
  /** Downloads, installs and restarts the app; emits `update-progress`. */
  installUpdate: () => call<void>("install_update"),
  webdavHasVault: (webdav: WebDavInput) => call<boolean>("webdav_has_vault", { webdav }),
  createWebdavVault: (webdav: WebDavInput, password: string) => call<string>("create_webdav_vault", { webdav, password }),
  connectWebdavVault: (webdav: WebDavInput, password: string) => call<void>("connect_webdav_vault", { webdav, password }),
  /** `null` turns backups off. */
  setBackupDir: (path: string | null) => call<void>("set_backup_dir", { path }),
  backUpNow: () => call<number>("back_up_now"),
  /** Copies the vault to an empty folder and switches to it. Returns where it was. */
  moveVaultToFolder: (path: string) => call<string>("move_vault_to_folder", { path }),
  unlockWithTouchId: () => call<void>("unlock_with_touch_id"),
  setTouchIdUnlock: (enabled: boolean) => call<void>("set_touch_id_unlock", { enabled }),
  listDevices: () => call<DevicesView>("list_devices"),
  createEnrollmentCode: () => call<{ code: string; expiresAt: number }>("create_enrollment_code"),
  /** Copies the code just made; the clipboard clears like any copied secret. */
  copyEnrollmentCode: () => call<void>("copy_enrollment_code"),
  /** Re-keys the vault; returns the new recovery key. */
  revokeDevice: (deviceId: string, password: string) => call<string>("revoke_device", { deviceId, password }),
  connectWithCode: (where: { path: string } | { bucket: BucketInput }, code: string) =>
    call<void>("connect_with_code", { ...where, code }),
  /** `lifetime: null` denies. */
  answerAgentRequest: (id: string, lifetime: GrantLifetime | null) =>
    call<void>("answer_agent_request", { id, lifetime }),
  agentActivity: () => call<AgentActivity>("agent_activity"),
  revokeAgentGrants: () => call<void>("revoke_agent_grants"),
  /** Turning it off asks for Touch ID first. */
  setAgentApprovalAuth: (required: boolean) => call<void>("set_agent_approval_auth", { required }),
  listServices: () => call<ServiceView[]>("list_services"),
  listConnections: () => call<ConnectionView[]>("list_connections"),
  addConnection: (serviceId: string, name: string, settings: Record<string, string>, token: string) =>
    call<void>("add_connection", { serviceId, name, settings, token }),
  removeConnection: (id: string) => call<void>("remove_connection", { id }),
  replaceConnectionToken: (id: string, token: string) => call<void>("replace_connection_token", { id, token }),
  secretIntegrations: (secretId: string) => call<SecretIntegrations>("secret_integrations", { secretId }),
  /** `connectionId: null` removes the provider. */
  setSecretProvider: (secretId: string, connectionId: string | null, settings: Record<string, string>) =>
    call<void>("set_secret_provider", { secretId, connectionId, settings }),
  addSecretTarget: (secretId: string, connectionId: string, name: string, settings: Record<string, string>) =>
    call<void>("add_secret_target", { secretId, connectionId, name, settings }),
  removeSecretTarget: (secretId: string, targetId: string) => call<void>("remove_secret_target", { secretId, targetId }),
  /** `pasted`: the key made in a guided provider's console. */
  rotateWithProvider: (secretId: string, leaked: boolean, pasted: string | null = null) =>
    call<RotationView>("rotate_with_provider", { secretId, leaked, pasted }),
  openProviderConsole: (serviceId: string) => call<void>("open_provider_console", { serviceId }),
  pushSecret: (secretId: string) => call<PushView[]>("push_secret", { secretId }),
  /** Revokes the previous key at the provider, then retires it. */
  /** `byHand`: you revoked it in the provider's console; it's just retired. */
  revokePreviousWithProvider: (secretId: string, byHand = false) =>
    call<number>("revoke_previous_with_provider", { secretId, byHand }),
  /** `null` when nothing is queued. */
  rotationQueue: () => call<QueueView | null>("rotation_queue"),
  rotateQueuedWithProviders: () =>
    call<{ name: string; error: string | null; pushes: PushView[] }[]>("rotate_queued_with_providers"),
  dismissRotationQueue: () => call<void>("dismiss_rotation_queue"),
  listProxyProfiles: () => call<ProxyProfileView[]>("list_proxy_profiles"),
  addProxyProfile: (profile: ProxyProfileInput) => call<void>("add_proxy_profile", { profile }),
  removeProxyProfile: (id: string) => call<void>("remove_proxy_profile", { id }),
  getSettings: () => call<Settings>("get_settings"),
  activateLicense: (key: string) => call<License>("activate_license", { key }),
  checkLicense: () => call<License | null>("check_license"),
  /** `force` forgets the license even if the server can't be told. */
  removeLicense: (force = false) => call<void>("remove_license", { force }),
  openPurchasePage: () => call<void>("open_purchase_page"),
  openDocs: (page: "agents" | "install") => call<void>("open_docs", { page }),
  cliStatus: () => call<CliStatus>("cli_status"),
  /** False when the admin password prompt was cancelled. */
  installCli: () => call<boolean>("install_cli"),
};

/** The CLI bundled in the app, and whether /usr/local/bin/dotsafe points at it. */
export type CliStatus = {
  /** This build carries the CLI (release builds on macOS). */
  available: boolean;
  installed: boolean;
  /** Something else is at the path: an older manual install, or Homebrew. */
  otherInstall: boolean;
  path: string;
};

export function message(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}
