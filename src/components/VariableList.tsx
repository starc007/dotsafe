import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { AnimatePresence, motion } from "motion/react";
import { toast } from "sonner";
import { Braces, Check, Copy, CornerDownRight, Eye, EyeOff, FileUp, KeyRound, Link2, Pencil, Plus, RotateCcw, Search, Share2, Trash2, Unlink, Upload } from "lucide-react";
import { api, message, type EnvironmentView, type ImportResult, type SecretRef, type VariableView } from "@/lib/api";
import { base, fast } from "@/lib/motion";
import { SHORTCUTS, ariaShortcut, useShortcut, type Shortcut } from "@/lib/shortcuts";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { IconButton } from "@/components/IconButton";
import { ImportDialog } from "@/components/ImportDialog";
import { ShareDialog } from "@/components/ShareDialog";
import { ShortcutKeys } from "@/components/ShortcutKeys";
import { ActionSwapIcon, ActionSwapText } from "@/components/motion/action-swap";
import { Surface, SurfaceBody, SurfaceDescription, SurfaceHeader, SurfaceTitle } from "@/components/Surface";
import { VariableDialog } from "@/components/VariableDialog";

/** Revealed values hide again after this long. */
const REVEAL_MS = 30_000;
const MASK = "••••••••••••";
/** How long the copy button shows a check after copying. */
const COPIED_MS = 1500;
/** Rows grow in and collapse out when a variable is added or deleted. While
 * filtering (`instant`), rows appear and disappear immediately: animating many
 * heights at once on every keystroke feels laggy. */
const rowVariants = {
  hidden: (instant: boolean) => ({ opacity: 0, height: 0, transition: instant ? { duration: 0 } : base }),
  shown: { opacity: 1, height: "auto", transition: base },
};

type Dialog =
  | { type: "add" }
  | { type: "edit"; key: string; value: string }
  | { type: "import"; droppedFiles?: string[] }
  | null;

export function VariableList({
  environment,
  projectName,
  onChanged,
}: {
  environment: EnvironmentView;
  projectName: string;
  onChanged: () => Promise<void>;
}) {
  const envId = environment.id;
  const label = `${projectName}/${environment.name}`;
  const [keys, setKeys] = useState<string[] | null>(null);
  // Variables linked to a shared secret, by name.
  const [links, setLinks] = useState<Record<string, SecretRef>>({});
  // Where each variable comes from (own, inherited, removed here), by name.
  const [views, setViews] = useState<Record<string, VariableView>>({});
  const [error, setError] = useState<string | null>(null);
  const [revealed, setRevealed] = useState<Record<string, string>>({});
  const [dialog, setDialog] = useState<Dialog>(null);
  const [deleting, setDeleting] = useState<string | null>(null);
  const [sharing, setSharing] = useState<string | null>(null);
  const [unlinking, setUnlinking] = useState<string | null>(null);
  // Keeps the share dialog's variable while it animates closed.
  const lastShared = useRef("");
  if (sharing) lastShared.current = sharing;
  const [filter, setFilter] = useState("");
  const [copiedKey, setCopiedKey] = useState<string | null>(null);
  const [dragging, setDragging] = useState(false);
  // True while the list changes because of the filter, so rows don't animate.
  const [instant, setInstant] = useState(false);
  // ⌘F shows the filter even on short lists, where it's normally hidden.
  const [filterOpen, setFilterOpen] = useState(false);
  const filterInput = useRef<HTMLInputElement>(null);
  const timers = useRef(new Map<string, number>());
  const lastVarDialog = useRef<Extract<Dialog, { type: "add" | "edit" }> | null>(null);
  const lastImportDialog = useRef<Extract<Dialog, { type: "import" }> | null>(null);

  const load = useCallback(async () => {
    setInstant(false);
    try {
      const vars = await api.listVariables(envId);
      setKeys(vars.map((v) => v.key));
      setLinks(Object.fromEntries(vars.flatMap((v) => (v.secret ? [[v.key, v.secret]] : []))));
      setViews(Object.fromEntries(vars.map((v) => [v.key, v])));
      setError(null);
    } catch (e) {
      setError(message(e));
    }
  }, [envId]);

  useEffect(() => {
    load();
    const unlisten = listen("vault-synced", load);
    const t = timers.current;
    return () => {
      unlisten.then((f) => f());
      t.forEach((id) => window.clearTimeout(id));
    };
  }, [load]);

  // Dropping .env files anywhere on the window opens the import dialog. Rust
  // keeps the paths and tells us only the file names.
  useEffect(() => {
    const unlistenDrop = listen<string[]>("env-files-dropped", (e) => {
      setDragging(false);
      setDialog({ type: "import", droppedFiles: e.payload });
    });
    let unlistenHover: Promise<() => void> | undefined;
    try {
      unlistenHover = getCurrentWebview().onDragDropEvent((e) => {
        if (e.payload.type === "enter" || e.payload.type === "over") setDragging(true);
        else setDragging(false);
      });
    } catch {
      // Not in a Tauri window (browser dev mode): no file drops.
    }
    return () => {
      unlistenDrop.then((f) => f());
      unlistenHover?.then((f) => f());
    };
  }, []);

  const openAdd = () => setDialog({ type: "add" });
  const openImport = () => setDialog({ type: "import" });
  useShortcut(SHORTCUTS.addVariable, openAdd);
  useShortcut(SHORTCUTS.importVariables, openImport);
  useShortcut(SHORTCUTS.filterVariables, () => {
    if (!keys?.length) return;
    setFilterOpen(true);
    // Focus once the input exists (it may only now be rendered).
    requestAnimationFrame(() => filterInput.current?.focus());
  });

  function hide(key: string) {
    window.clearTimeout(timers.current.get(key));
    timers.current.delete(key);
    setRevealed(({ [key]: _, ...rest }) => rest);
  }

  async function toggleReveal(key: string) {
    if (key in revealed) return hide(key);
    try {
      const value = await api.revealVariable(envId, key);
      setRevealed((r) => ({ ...r, [key]: value }));
      timers.current.set(key, window.setTimeout(() => hide(key), REVEAL_MS));
    } catch (e) {
      toast.error(message(e));
    }
  }

  async function copy(key: string) {
    try {
      await api.copyVariable(envId, key);
      setCopiedKey(key);
      window.setTimeout(() => setCopiedKey((k) => (k === key ? null : k)), COPIED_MS);
      toast(`Copied ${key}`, { description: "The clipboard clears in 30 seconds." });
    } catch (e) {
      toast.error(message(e));
    }
  }

  async function startEdit(key: string) {
    try {
      // As written, so `${{ NAME }}` references survive the edit.
      const value = await api.revealVariable(envId, key, true);
      setDialog({ type: "edit", key, value });
    } catch (e) {
      toast.error(message(e));
    }
  }

  async function saved(key: string, previousKey?: string) {
    setDialog(null);
    if (previousKey) hide(previousKey);
    hide(key);
    await load();
    await onChanged();
    toast.success(previousKey ? `Saved ${key}` : `Added ${key}`);
  }

  async function imported(result: ImportResult) {
    setDialog(null);
    setRevealed({});
    await load();
    await onChanged();
    const parts = [
      result.added && `${result.added} added`,
      result.updated && `${result.updated} updated`,
      result.skipped && `${result.skipped} kept as they were`,
    ].filter(Boolean);
    toast.success(`Saved to ${label}`, { description: parts.join(" · ") || "Everything was already up to date." });
  }

  async function shared(key: string, name: string, linked: number) {
    setSharing(null);
    hide(key);
    await load();
    await onChanged();
    toast.success(name === key ? `${key} is now shared` : `${key} is now the shared secret ${name}`, {
      description: linked ? `Also linked in ${linked} other ${linked === 1 ? "environment" : "environments"}.` : undefined,
    });
  }

  async function unlinkConfirmed() {
    if (!unlinking) return;
    try {
      await api.unlinkVariable(envId, unlinking);
      toast(`${unlinking} has its own value now`, { description: "Rotating the shared secret won't change it." });
      await load();
    } catch (e) {
      toast.error(message(e));
    } finally {
      setUnlinking(null);
    }
  }

  /** Removes an inherited variable here, or brings it back. */
  async function setRemoved(key: string, removed: boolean) {
    try {
      if (removed) await api.hideVariable(envId, key);
      else await api.restoreVariable(envId, key);
      hide(key);
      toast(removed ? `${key} is removed here` : `${key} is inherited again`);
      await load();
    } catch (e) {
      toast.error(message(e));
    }
  }

  async function deleteConfirmed() {
    if (!deleting) return;
    try {
      await api.deleteVariable(envId, deleting);
      hide(deleting);
      toast(`Deleted ${deleting}`);
      await load();
      await onChanged();
    } catch (e) {
      toast.error(message(e));
    } finally {
      setDeleting(null);
    }
  }

  if (error) {
    return (
      <div className="flex flex-1 flex-col items-center justify-center gap-3 text-center">
        <p className="text-sm font-semibold">Couldn't load variables</p>
        <p className="selectable max-w-sm text-muted-foreground">{error}</p>
        <Button variant="outline" onClick={load}>
          Try again
        </Button>
      </div>
    );
  }

  // Dialogs keep showing their last contents while they animate closed.
  const varDialog = dialog?.type === "add" || dialog?.type === "edit" ? dialog : null;
  if (varDialog) lastVarDialog.current = varDialog;
  if (dialog?.type === "import") lastImportDialog.current = dialog;
  const shownVar = lastVarDialog.current;

  const visible = keys?.filter((k) => k.toLowerCase().includes(filter.trim().toLowerCase())) ?? [];
  const own = keys?.filter((k) => !views[k]?.inheritedFrom).length ?? 0;
  const inherited = keys?.filter((k) => views[k]?.inheritedFrom && !views[k]?.removed).length ?? 0;
  const parentName = keys?.map((k) => views[k]?.inheritedFrom ?? views[k]?.overrides).find(Boolean);


  return (
    <div className="relative min-h-0 flex-1 overflow-y-auto p-4">
      <Surface>
        <SurfaceHeader
          icon={<KeyRound aria-hidden />}
          action={
            <div className="flex items-center gap-2">
              {keys && keys.length > 0 && (keys.length > 6 || filterOpen) && (
                <div className="relative w-40">
                  <Search aria-hidden className="pointer-events-none absolute top-2 left-2.5 size-4 text-muted-foreground" />
                  <Input
                    ref={filterInput}
                    aria-label="Filter variables"
                    aria-keyshortcuts={ariaShortcut(SHORTCUTS.filterVariables)}
                    placeholder="Filter"
                    value={filter}
                    onChange={(e) => {
                      setInstant(true);
                      setFilter(e.target.value);
                    }}
                    onKeyDown={(e) => {
                      if (e.key !== "Escape") return;
                      e.stopPropagation();
                      if (filter) {
                        setInstant(true);
                        setFilter("");
                      } else {
                        setFilterOpen(false);
                        e.currentTarget.blur();
                      }
                    }}
                    className="bg-card pl-8"
                  />
                </div>
              )}
              {/* An empty environment shows these in its own empty state instead. */}
              {keys && keys.length > 0 && (
                <>
                  <WithShortcut shortcut={SHORTCUTS.importVariables}>
                    <Button variant="outline" onClick={openImport} className="bg-card">
                      <Upload aria-hidden />
                      Import
                    </Button>
                  </WithShortcut>
                  <WithShortcut shortcut={SHORTCUTS.addVariable}>
                    <Button onClick={openAdd}>
                      <Plus aria-hidden />
                      Add variable
                    </Button>
                  </WithShortcut>
                </>
              )}
            </div>
          }
        >
          <SurfaceTitle>{environment.name}</SurfaceTitle>
          <SurfaceDescription>
            {keys === null
              ? " "
              : `${own} ${own === 1 ? "variable" : "variables"}${
                  inherited ? ` · ${inherited} inherited${parentName ? ` from ${parentName}` : ""}` : ""
                }`}
          </SurfaceDescription>
        </SurfaceHeader>

        <SurfaceBody className="overflow-hidden p-0">
          {keys === null ? (
            <RowsSkeleton />
          ) : keys.length === 0 ? (
            <EmptyState label={label} onImport={openImport} onAdd={openAdd} />
          ) : (
            <ul aria-label="Variables" className="divide-y">
              <AnimatePresence initial={false} custom={instant}>
                {visible.map((key) => {
                  const view = views[key];
                  const from = view?.inheritedFrom;
                  if (view?.removed)
                    return (
                      <motion.li
                        key={key}
                        custom={instant}
                        variants={rowVariants}
                        initial={instant ? false : "hidden"}
                        animate="shown"
                        exit="hidden"
                        className="group flex min-h-11 items-center gap-3 overflow-hidden px-4 py-1.5"
                      >
                        <span className="w-2/5 max-w-56 shrink-0 truncate font-mono text-[12.5px] text-muted-foreground line-through" title={key}>
                          {key}
                        </span>
                        <span className="min-w-0 flex-1 truncate text-[13px] text-muted-foreground">
                          Removed here; {from} has it
                        </span>
                        <IconButton label={`Inherit ${key} from ${from} again`} onClick={() => setRemoved(key, false)}>
                          <RotateCcw aria-hidden />
                        </IconButton>
                      </motion.li>
                    );
                  return (
                  <motion.li
                    key={key}
                    custom={instant}
                    variants={rowVariants}
                    initial={instant ? false : "hidden"}
                    animate="shown"
                    exit="hidden"
                    className="group flex min-h-11 items-center gap-3 overflow-hidden px-4 py-1.5"
                  >
                    <span
                      className={`selectable flex w-2/5 max-w-56 shrink-0 items-center gap-1.5 truncate font-mono text-[12.5px] font-medium ${from ? "text-muted-foreground" : ""}`}
                      title={key}
                    >
                      <span className="truncate">{key}</span>
                      {view?.references && (
                        <Tooltip>
                          <TooltipTrigger asChild>
                            <Braces aria-label="Uses references" className="size-3.5 shrink-0 text-muted-foreground" />
                          </TooltipTrigger>
                          <TooltipContent>Includes other variables with {"${{ NAME }}"}; revealing shows the result.</TooltipContent>
                        </Tooltip>
                      )}
                    </span>
                    {(from || view?.overrides) && (
                      <span className="flex shrink-0 items-center gap-1 rounded-full bg-muted px-2 py-0.5 text-[11px] text-muted-foreground">
                        <CornerDownRight aria-hidden className="size-3" />
                        {from ? `from ${from}` : `overrides ${view?.overrides}`}
                      </span>
                    )}
                    {links[key] && (
                      <Tooltip>
                        <TooltipTrigger asChild>
                          <button
                            type="button"
                            onClick={() => window.dispatchEvent(new CustomEvent("dotsafe:open-secret", { detail: links[key].id }))}
                            className="flex max-w-40 shrink-0 items-center gap-1 rounded-full bg-primary/10 px-2 py-0.5 text-[11px] font-medium text-primary outline-none hover:bg-primary/15 focus-visible:ring-2 focus-visible:ring-ring"
                          >
                            <Link2 aria-hidden className="size-3 shrink-0" />
                            <span className="truncate">{links[key].name}</span>
                          </button>
                        </TooltipTrigger>
                        <TooltipContent>Shared secret. Open it to rotate or see where it's used.</TooltipContent>
                      </Tooltip>
                    )}
                    {/* Revealing rolls the mask away and the value up into place. */}
                    <span className="min-w-0 flex-1">
                      <ActionSwapText
                        value={key in revealed ? "value" : "mask"}
                        animation="roll"
                        className="font-mono text-[12.5px]"
                      >
                        <span
                          className={key in revealed ? "selectable" : "tracking-widest text-muted-foreground"}
                          title={key in revealed ? revealed[key] : undefined}
                        >
                          {key in revealed
                            ? revealed[key] || <em className="font-sans text-muted-foreground">empty</em>
                            : MASK}
                        </span>
                      </ActionSwapText>
                    </span>
                    {/* Kept on its own compositing layer (will-change): WebKit otherwise
                        fails to repaint plain icons when the fade ends next to the
                        animated swap icons, leaving stale buttons visible. */}
                    <div className="flex shrink-0 items-center gap-0.5 opacity-0 transition-opacity will-change-[opacity] group-focus-within:opacity-100 group-hover:opacity-100">
                      <IconButton label={key in revealed ? `Hide ${key}` : `Reveal ${key}`} onClick={() => toggleReveal(key)}>
                        <ActionSwapIcon value={key in revealed ? "hide" : "reveal"} animation="blur" className="size-4">
                          {key in revealed ? <EyeOff aria-hidden /> : <Eye aria-hidden />}
                        </ActionSwapIcon>
                      </IconButton>
                      <IconButton label={`Copy ${key}`} onClick={() => copy(key)}>
                        <ActionSwapIcon value={copiedKey === key ? "copied" : "copy"} animation="blur" className="size-4">
                          {copiedKey === key ? <Check aria-hidden /> : <Copy aria-hidden />}
                        </ActionSwapIcon>
                      </IconButton>
                      {from ? (
                        <>
                          <IconButton label={`Override ${key} here`} onClick={() => startEdit(key)}>
                            <Pencil aria-hidden />
                          </IconButton>
                          <IconButton label={`Remove ${key} here`} onClick={() => setRemoved(key, true)} className="hover:text-destructive">
                            <Trash2 aria-hidden />
                          </IconButton>
                        </>
                      ) : links[key] ? (
                        <IconButton label={`Unlink ${key} from ${links[key].name}`} onClick={() => setUnlinking(key)}>
                          <Unlink aria-hidden />
                        </IconButton>
                      ) : (
                        <>
                          <IconButton label={`Share ${key}`} onClick={() => setSharing(key)}>
                            <Share2 aria-hidden />
                          </IconButton>
                          <IconButton label={`Edit ${key}`} onClick={() => startEdit(key)}>
                            <Pencil aria-hidden />
                          </IconButton>
                        </>
                      )}
                      {!from && (
                        <IconButton label={`Delete ${key}`} onClick={() => setDeleting(key)} className="hover:text-destructive">
                          <Trash2 aria-hidden />
                        </IconButton>
                      )}
                    </div>
                  </motion.li>
                  );
                })}
              </AnimatePresence>
              {visible.length === 0 && (
                <li className="px-4 py-6 text-center text-muted-foreground">No variables match “{filter}”.</li>
              )}
            </ul>
          )}
        </SurfaceBody>
      </Surface>

      <AnimatePresence>
        {dragging && (
          <motion.div
            key="drop"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={fast}
            className="pointer-events-none absolute inset-4 flex flex-col items-center justify-center gap-2 rounded-3xl bg-primary/10 text-primary backdrop-blur-[2px]"
          >
            <FileUp aria-hidden className="size-6" />
            <p className="font-medium">Drop to import into {label}</p>
          </motion.div>
        )}
      </AnimatePresence>

      <VariableDialog
        open={varDialog !== null}
        onOpenChange={(open) => !open && setDialog(null)}
        environmentId={envId}
        environmentLabel={label}
        initial={shownVar?.type === "edit" ? { key: shownVar.key, value: shownVar.value } : undefined}
        onSaved={saved}
        onImported={imported}
      />
      <ImportDialog
        open={dialog?.type === "import"}
        onOpenChange={(open) => !open && setDialog(null)}
        environmentId={envId}
        environmentLabel={label}
        droppedFiles={lastImportDialog.current?.droppedFiles}
        onImported={imported}
      />
      <ShareDialog
        open={sharing !== null}
        onOpenChange={(open) => !open && setSharing(null)}
        environmentId={envId}
        variable={lastShared.current}
        onShared={(name, linked) => shared(lastShared.current, name, linked)}
      />
      <ConfirmDialog
        open={unlinking !== null}
        title={`Unlink ${unlinking}?`}
        description={`It keeps the current value, but stops following ${unlinking ? links[unlinking]?.name : "the secret"} when it's rotated.`}
        confirmLabel="Unlink"
        onConfirm={unlinkConfirmed}
        onCancel={() => setUnlinking(null)}
      />
      <ConfirmDialog
        open={deleting !== null}
        title={`Delete ${deleting}?`}
        description={
          deleting && views[deleting]?.overrides
            ? `${label}'s own value is removed; ${views[deleting]?.overrides}'s applies again.`
            : `It will be removed from ${label} on every synced device.`
        }
        confirmLabel="Delete"
        onConfirm={deleteConfirmed}
        onCancel={() => setDeleting(null)}
      />
    </div>
  );
}

/** Wraps a button with a tooltip showing its shortcut, and announces it. */
function WithShortcut({ shortcut, children }: { shortcut: Shortcut; children: React.ReactElement }) {
  return (
    <Tooltip>
      <TooltipTrigger asChild aria-keyshortcuts={ariaShortcut(shortcut)}>
        {children}
      </TooltipTrigger>
      <TooltipContent className="flex items-center gap-2">
        {shortcut.label}
        <ShortcutKeys shortcut={shortcut} />
      </TooltipContent>
    </Tooltip>
  );
}

function EmptyState({ label, onImport, onAdd }: { label: string; onImport: () => void; onAdd: () => void }) {
  return (
    <div className="flex flex-col items-center gap-3 px-6 py-12 text-center">
      <div className="flex size-11 items-center justify-center rounded-xl bg-muted text-muted-foreground">
        <FileUp aria-hidden className="size-5" />
      </div>
      <div>
        <p className="text-sm font-semibold">No variables in {label} yet</p>
        <p className="mt-1 max-w-sm text-muted-foreground">
          Bring in an existing .env file, or drop one anywhere in this window. You can also add variables one at a time.
        </p>
      </div>
      <div className="mt-1 flex gap-2">
        <Button onClick={onImport}>
          <Upload aria-hidden />
          Import .env
        </Button>
        <Button variant="outline" onClick={onAdd}>
          <Plus aria-hidden />
          Add variable
        </Button>
      </div>
    </div>
  );
}

function RowsSkeleton() {
  return (
    <div aria-hidden className="divide-y">
      {[0, 1, 2].map((i) => (
        <div key={i} className="flex h-11 items-center gap-3 px-4">
          <Skeleton className="h-3.5 w-40" />
          <Skeleton className="h-3.5 w-28" />
        </div>
      ))}
    </div>
  );
}
