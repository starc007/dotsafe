import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { listen } from "@tauri-apps/api/event";
import { AnimatePresence, motion } from "motion/react";
import { ArrowDownToLine, CloudOff, FolderLock, KeyRound, ListChecks, Lock, Plus, Search, Settings as SettingsIcon, TriangleAlert } from "lucide-react";
import {
  api,
  message,
  type ProjectView,
  type QueueView,
  type SyncView,
  type UpdateView,
  type VaultHealth,
} from "@/lib/api";
import { base, snappy } from "@/lib/motion";
import { SETTINGS_SECTIONS, type SettingsSection } from "@/lib/settingsSections";
import { SHORTCUTS, ariaShortcut, useShortcut } from "@/lib/shortcuts";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { DragRegion } from "@/components/DragRegion";
import { IconButton } from "@/components/IconButton";
import { InlineCreate } from "@/components/InlineCreate";
import { ProjectPane } from "@/components/ProjectPane";
import { QuickCopy } from "@/components/QuickCopy";
import { RotationQueueDialog } from "@/components/RotationQueueDialog";
import { UpdateDialog } from "@/components/UpdateDialog";
import { SecretDialog } from "@/components/SecretDialog";
import { SettingsNav } from "@/components/SettingsNav";
import { SidebarAction } from "@/components/SidebarAction";
import { SecretsPage } from "@/screens/Secrets";
import { SettingsPage } from "@/screens/Settings";

/** Slide between projects and settings. `custom` is the direction (see `direction`). */
const swapMotion = {
  variants: {
    enter: (d: number) => ({ x: d * 24, opacity: 0 }),
    center: { x: 0, opacity: 1 },
    exit: (d: number) => ({ x: d * -24, opacity: 0 }),
  },
  initial: "enter",
  animate: "center",
  exit: "exit",
  transition: base,
} as const;

export function VaultScreen({ onLocked }: { onLocked: () => void }) {
  const [projects, setProjects] = useState<ProjectView[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const [quickCopyOpen, setQuickCopyOpen] = useState(false);
  const [health, setHealth] = useState<VaultHealth | null>(null);
  const [queue, setQueue] = useState<QueueView | null>(null);
  const [queueOpen, setQueueOpen] = useState(false);
  // A bucket vault that couldn't sync; changes wait on this Mac.
  const [syncError, setSyncError] = useState<string | null>(null);
  // A newer version of the app; nothing downloads until asked.
  const [update, setUpdate] = useState<UpdateView | null>(null);
  const [updateOpen, setUpdateOpen] = useState(false);
  // The open settings section, or null when Settings is closed.
  const [settingsSection, setSettingsSection] = useState<SettingsSection | null>(null);
  const settingsOpen = settingsSection !== null;
  // Secrets, and the secret whose details are open there.
  const [secretsOpen, setSecretsOpen] = useState(false);
  const [openSecretId, setOpenSecretId] = useState<string | null>(null);
  // A secret opened from a variable's badge, over the project, without leaving it.
  const [badgeSecret, setBadgeSecret] = useState<{ id: string; open: boolean } | null>(null);
  // An environment to open in the selected project, when jumping from a secret.
  const [focusEnvironment, setFocusEnvironment] = useState<string | null>(null);
  // Reopening Settings returns to the section viewed last.
  const lastSection = useRef<SettingsSection>("license");
  if (settingsSection) lastSection.current = settingsSection;
  const setSettingsOpen = (open: boolean | ((open: boolean) => boolean)) =>
    setSettingsSection((current) => {
      const next = typeof open === "function" ? open(current !== null) : open;
      return next ? (current ?? lastSection.current) : null;
    });

  const load = useCallback(async () => {
    try {
      const list = await api.listProjects();
      setProjects(list);
      setError(null);
      setSelectedId((id) => (id && list.some((p) => p.id === id) ? id : (list[0]?.id ?? null)));
    } catch (e) {
      setError(message(e));
    }
    api.vaultHealth().then(setHealth, () => {});
    api.rotationQueue().then(setQueue, () => {});
    api.getSettings().then((s) => setSyncError(s.syncError), () => {});
  }, []);

  useEffect(() => {
    load();
    // Another device's changes arrived through the bucket.
    const unlisten = listen("vault-synced", load);
    // A device was removed, or something queued was rotated.
    window.addEventListener("dotsafe:vault-changed", load);
    // The menu bar icon or the global shortcut (⌥⌘K) asked for quick copy.
    const unlistenQuick = listen("open-quick-copy", () => setQuickCopyOpen(true));
    // Each background sync of a bucket vault reports how it went.
    const unlistenSync = listen<SyncView>("sync-status", (e) => setSyncError(e.payload.error));
    // The background check (release builds) found a newer version.
    const unlistenUpdate = listen<UpdateView>("update-available", (e) => setUpdate(e.payload));
    // Settings' "Check for updates" found one: show it now.
    const onFound = (e: Event) => {
      setUpdate((e as CustomEvent<UpdateView>).detail);
      setUpdateOpen(true);
    };
    window.addEventListener("dotsafe:update-found", onFound);
    return () => {
      unlisten.then((f) => f());
      unlistenQuick.then((f) => f());
      unlistenSync.then((f) => f());
      unlistenUpdate.then((f) => f());
      window.removeEventListener("dotsafe:update-found", onFound);
      window.removeEventListener("dotsafe:vault-changed", load);
    };
  }, [load]);

  async function createProject(name: string) {
    const project = await api.createProject(name);
    await load();
    setSelectedId(project.id);
    setCreating(false);
  }

  async function lock() {
    await api.lock();
    onLocked();
  }

  function selectProject(id: string) {
    setSelectedId(id);
    setSettingsOpen(false);
    setSecretsOpen(false);
    setFocusEnvironment(null);
  }

  function openSecrets(secretId: string | null = null) {
    setSettingsOpen(false);
    setSecretsOpen(true);
    setOpenSecretId(secretId);
  }

  const openEnvironment = useCallback((projectId: string, environmentId: string) => {
    setSettingsSection(null);
    setOpenSecretId(null);
    setSecretsOpen(false);
    setSelectedId(projectId);
    setFocusEnvironment(environmentId);
  }, []);

  // A secret's Integrations tab asks to connect an account.
  useEffect(() => {
    const onOpen = () => {
      setBadgeSecret((b) => b && { ...b, open: false });
      setOpenSecretId(null);
      setSecretsOpen(false);
      setSettingsSection("integrations");
    };
    window.addEventListener("dotsafe:open-integrations", onOpen);
    return () => window.removeEventListener("dotsafe:open-integrations", onOpen);
  }, []);

  // A linked variable's badge asks to open its secret: in place, over the project.
  useEffect(() => {
    const onOpen = (e: Event) => setBadgeSecret({ id: (e as CustomEvent<string>).detail, open: true });
    window.addEventListener("dotsafe:open-secret", onOpen);
    return () => window.removeEventListener("dotsafe:open-secret", onOpen);
  });

  function startCreating() {
    setSettingsOpen(false);
    setCreating(true);
  }

  function stepProject(delta: number) {
    // In Settings, the same keys move between sections.
    if (settingsSection) {
      const at = SETTINGS_SECTIONS.findIndex((s) => s.id === settingsSection);
      const n = SETTINGS_SECTIONS.length;
      setSettingsSection(SETTINGS_SECTIONS[(at + delta + n) % n].id);
      return;
    }
    if (!projects?.length) return;
    const at = projects.findIndex((p) => p.id === selectedId);
    selectProject(projects[(at + delta + projects.length) % projects.length].id);
  }

  useShortcut(SHORTCUTS.quickCopy, () => setQuickCopyOpen(true));
  useShortcut(SHORTCUTS.lock, lock);
  useShortcut(SHORTCUTS.settings, () => {
    setSecretsOpen(false);
    setSettingsOpen((open) => !open);
  });
  useShortcut(SHORTCUTS.secrets, () => (secretsOpen ? setSecretsOpen(false) : openSecrets()));
  useShortcut(SHORTCUTS.newProject, startCreating);
  useShortcut(SHORTCUTS.previousProject, () => stepProject(-1));
  useShortcut(SHORTCUTS.nextProject, () => stepProject(1));

  const selected = projects?.find((p) => p.id === selectedId) ?? null;
  // New content enters from the right going into Settings or Secrets,
  // from the left coming back.
  const direction = settingsOpen || secretsOpen ? 1 : -1;

  return (
    <div className="flex h-full">
      <aside className="flex w-56 shrink-0 flex-col border-r border-sidebar-border bg-sidebar text-sidebar-foreground">
        {/* Space for the macOS traffic lights; also drags the window. */}
        <DragRegion className="h-12 shrink-0" />
        {/* Projects and settings slide past each other: into Settings to the left,
            back to projects to the right. */}
        <div className="relative flex min-h-0 flex-1 flex-col overflow-hidden">
          <AnimatePresence mode="popLayout" initial={false} custom={direction}>
            {settingsSection ? (
              <motion.div key="settings" custom={direction} {...swapMotion} className="flex min-h-0 flex-1 flex-col">
                <SettingsNav section={settingsSection} onSelect={setSettingsSection} onBack={() => setSettingsOpen(false)} />
              </motion.div>
            ) : (
              <motion.div key="projects" custom={direction} {...swapMotion} className="flex min-h-0 flex-1 flex-col">
            <div className="flex items-center justify-between px-3 pt-1 pb-1">
              <h2 className="px-1 text-xs font-medium text-muted-foreground">Projects</h2>
              <IconButton label="New project" aria-keyshortcuts={ariaShortcut(SHORTCUTS.newProject)} onClick={startCreating}>
                <Plus aria-hidden />
              </IconButton>
            </div>
            <nav aria-label="Projects" className="flex-1 overflow-y-auto px-2 pt-0.5 pb-2">
              {creating && (
                <InlineCreate
                  label="Project name"
                  placeholder="my-app"
                  className="mb-1"
                  onCreate={createProject}
                  onCancel={() => setCreating(false)}
                />
              )}
              {projects === null && !error && <SidebarSkeleton />}
              <AnimatePresence initial={false}>
                {projects?.map((p) => {
                  const current = p.id === selectedId && !settingsOpen && !secretsOpen;
                  return (
                    <motion.button
                      key={p.id}
                      layout="position"
                      initial={{ opacity: 0, y: -4 }}
                      animate={{ opacity: 1, y: 0 }}
                      exit={{ opacity: 0, transition: { duration: 0.12 } }}
                      transition={base}
                      aria-current={current ? "page" : undefined}
                      onClick={() => selectProject(p.id)}
                      className={cn(
                        "relative flex h-8 w-full items-center rounded-md px-2 text-left outline-none transition-colors",
                        "hover:bg-sidebar-accent/60 focus-visible:ring-2 focus-visible:ring-sidebar-ring",
                        "aria-[current=page]:font-medium aria-[current=page]:text-sidebar-accent-foreground",
                      )}
                    >
                      {/* One highlight that slides between projects as the selection moves. */}
                      {current && (
                        <motion.span
                          layoutId="project-highlight"
                          aria-hidden
                          className="absolute inset-0 rounded-md bg-sidebar-accent"
                          transition={snappy}
                        />
                      )}
                      <span className="relative truncate">{p.name}</span>
                    </motion.button>
                  );
                })}
              </AnimatePresence>
            </nav>
              </motion.div>
            )}
          </AnimatePresence>
        </div>
        <div className="flex flex-col gap-0.5 border-t border-sidebar-border p-2">
          {health && (health.stale > 0 || health.unreadable > 0 || health.damaged > 0) && (
            <button
              type="button"
              onClick={() => {
                setSecretsOpen(false);
                setSettingsSection("security");
              }}
              className="flex h-8 w-full items-center gap-2 rounded-md px-2 text-left font-medium text-destructive outline-none transition-colors hover:bg-sidebar-accent focus-visible:ring-2 focus-visible:ring-sidebar-ring [&_svg]:size-4 [&_svg]:shrink-0"
            >
              <TriangleAlert aria-hidden />
              <span className="flex-1 truncate">Vault needs a look</span>
            </button>
          )}
          {update && (
            <button
              type="button"
              onClick={() => setUpdateOpen(true)}
              className="flex h-8 w-full items-center gap-2 rounded-md px-2 text-left font-medium outline-none transition-colors hover:bg-sidebar-accent focus-visible:ring-2 focus-visible:ring-sidebar-ring [&_svg]:size-4 [&_svg]:shrink-0"
            >
              <ArrowDownToLine aria-hidden className="text-primary-ink" />
              <span className="flex-1 truncate">Update available</span>
              <span className="text-xs text-muted-foreground tabular-nums">{update.version}</span>
            </button>
          )}
          {syncError && (
            <button
              type="button"
              title={syncError}
              onClick={() => {
                setSecretsOpen(false);
                setSettingsSection("security");
              }}
              className="flex h-8 w-full items-center gap-2 rounded-md px-2 text-left font-medium outline-none transition-colors hover:bg-sidebar-accent focus-visible:ring-2 focus-visible:ring-sidebar-ring [&_svg]:size-4 [&_svg]:shrink-0"
            >
              <CloudOff aria-hidden />
              <span className="flex-1 truncate">{syncError.startsWith("Offline") ? "Offline" : "Sync failed"}</span>
              <span className="text-xs text-muted-foreground">Saved here</span>
            </button>
          )}
          {queue && (
            <button
              type="button"
              onClick={() => setQueueOpen(true)}
              className="flex h-8 w-full items-center gap-2 rounded-md px-2 text-left font-medium outline-none transition-colors hover:bg-sidebar-accent focus-visible:ring-2 focus-visible:ring-sidebar-ring [&_svg]:size-4 [&_svg]:shrink-0"
            >
              <ListChecks aria-hidden />
              <span className="flex-1 truncate">
                {queue.secrets.length + queue.variables.length > 0 ? "Rotate after removal" : "Rotation done"}
              </span>
              {queue.secrets.length + queue.variables.length > 0 && (
                <span className="text-xs text-muted-foreground">{queue.secrets.length + queue.variables.length}</span>
              )}
            </button>
          )}
          <SidebarAction
            icon={<Search aria-hidden />}
            label="Find"
            shortcut={SHORTCUTS.quickCopy}
            onClick={() => setQuickCopyOpen(true)}
          />
          <SidebarAction
            icon={<KeyRound aria-hidden />}
            label="Secrets"
            shortcut={SHORTCUTS.secrets}
            active={secretsOpen}
            onClick={() => (secretsOpen ? setSecretsOpen(false) : openSecrets())}
          />
          <SidebarAction
            icon={<SettingsIcon aria-hidden />}
            label="Settings"
            shortcut={SHORTCUTS.settings}
            active={settingsOpen}
            onClick={() => {
              setSecretsOpen(false);
              setSettingsOpen((open) => !open);
            }}
          />
          <SidebarAction icon={<Lock aria-hidden />} label="Lock" shortcut={SHORTCUTS.lock} onClick={lock} />
        </div>
      </aside>

      <section className="relative flex min-w-0 flex-1 flex-col overflow-hidden">
        <AnimatePresence mode="popLayout" initial={false} custom={direction}>
          {settingsSection ? (
            <motion.div key="settings" custom={direction} {...swapMotion} className="flex min-h-0 flex-1 flex-col">
              <SettingsPage section={settingsSection} onClose={() => setSettingsOpen(false)} onLock={lock} />
            </motion.div>
          ) : secretsOpen ? (
            <motion.div key="secrets" custom={direction} {...swapMotion} className="flex min-h-0 flex-1 flex-col">
              <SecretsPage
                openSecretId={openSecretId}
                onOpenSecret={setOpenSecretId}
                onClose={() => setSecretsOpen(false)}
                onOpenEnvironment={openEnvironment}
              />
            </motion.div>
          ) : (
            <motion.div key="projects" custom={direction} {...swapMotion} className="flex min-h-0 flex-1 flex-col">
              {error ? (
                <Message title="Couldn't load your projects" body={error}>
                  <Button variant="outline" onClick={load}>
                    Try again
                  </Button>
                </Message>
              ) : projects === null ? (
                <DragRegion className="h-12" />
              ) : selected ? (
                <ProjectPane
                  key={`${selected.id}:${focusEnvironment ?? ""}`}
                  project={selected}
                  initialEnvironmentId={focusEnvironment}
                  onChanged={load}
                />
              ) : (
                <Message
                  icon={<FolderLock aria-hidden className="size-5" />}
                  title="No projects yet"
                  body="A project holds the variables for one app, split into environments like development and production."
                >
                  <Button onClick={startCreating}>
                    <Plus aria-hidden />
                    New project
                  </Button>
                </Message>
              )}
            </motion.div>
          )}
        </AnimatePresence>
      </section>
      <SecretDialog
        secretId={badgeSecret?.id ?? null}
        open={badgeSecret?.open ?? false}
        // Keep the id while it closes, so the content doesn't vanish mid-fade.
        onOpenChange={(open) => !open && setBadgeSecret((b) => b && { ...b, open: false })}
        onChanged={() => {
          load();
          window.dispatchEvent(new Event("dotsafe:variables-changed"));
        }}
        onOpenEnvironment={(projectId, environmentId) => {
          setBadgeSecret((b) => b && { ...b, open: false });
          openEnvironment(projectId, environmentId);
        }}
      />
      {update && <UpdateDialog update={update} open={updateOpen} onOpenChange={setUpdateOpen} />}
      <RotationQueueDialog
        open={queueOpen}
        onOpenChange={setQueueOpen}
        onChanged={load}
        onOpenSecret={(id) => openSecrets(id)}
        onOpenEnvironment={openEnvironment}
        onOpenIntegrations={() => {
          setSecretsOpen(false);
          setSettingsSection("integrations");
        }}
      />
      <QuickCopy
        open={quickCopyOpen}
        onOpenChange={setQuickCopyOpen}
        projects={projects ?? []}
        onOpenEnvironment={openEnvironment}
      />
    </div>
  );
}

function Message({ icon, title, body, children }: { icon?: ReactNode; title: string; body: string; children?: ReactNode }) {
  return (
    <div className="relative flex flex-1 flex-col items-center justify-center gap-2 p-8 text-center">
      <DragRegion className="absolute inset-x-0 top-0 h-12" />
      {icon && <div className="mb-1 rounded-xl bg-muted p-3 text-muted-foreground">{icon}</div>}
      <h2 className="text-sm font-semibold">{title}</h2>
      <p className="selectable max-w-sm text-muted-foreground">{body}</p>
      {children && <div className="mt-3">{children}</div>}
    </div>
  );
}

function SidebarSkeleton() {
  return (
    <div aria-hidden className="flex flex-col gap-2 px-2 pt-1">
      <Skeleton className="h-5 w-3/4" />
      <Skeleton className="h-5 w-1/2" />
      <Skeleton className="h-5 w-4/5" />
    </div>
  );
}
