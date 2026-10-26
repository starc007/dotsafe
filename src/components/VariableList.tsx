import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { AnimatePresence, motion } from "motion/react";
import { toast } from "sonner";
import { Check, Copy, Eye, EyeOff, FileUp, KeyRound, Pencil, Plus, Search, Trash2, Upload } from "lucide-react";
import { api, message, type EnvironmentView, type ImportResult } from "@/lib/api";
import { base, fast } from "@/lib/motion";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { IconButton } from "@/components/IconButton";
import { IconSwap } from "@/components/IconSwap";
import { ImportDialog } from "@/components/ImportDialog";
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
  const [error, setError] = useState<string | null>(null);
  const [revealed, setRevealed] = useState<Record<string, string>>({});
  const [dialog, setDialog] = useState<Dialog>(null);
  const [deleting, setDeleting] = useState<string | null>(null);
  const [filter, setFilter] = useState("");
  const [copiedKey, setCopiedKey] = useState<string | null>(null);
  const [dragging, setDragging] = useState(false);
  // True while the list changes because of the filter, so rows don't animate.
  const [instant, setInstant] = useState(false);
  const timers = useRef(new Map<string, number>());
  const lastVarDialog = useRef<Extract<Dialog, { type: "add" | "edit" }> | null>(null);
  const lastImportDialog = useRef<Extract<Dialog, { type: "import" }> | null>(null);

  const load = useCallback(async () => {
    setInstant(false);
    try {
      setKeys(await api.listVariables(envId));
      setError(null);
    } catch (e) {
      setError(message(e));
    }
  }, [envId]);

  useEffect(() => {
    load();
    const t = timers.current;
    return () => t.forEach((id) => window.clearTimeout(id));
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
      const value = revealed[key] ?? (await api.revealVariable(envId, key));
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
  const openAdd = () => setDialog({ type: "add" });
  const openImport = () => setDialog({ type: "import" });

  return (
    <div className="relative min-h-0 flex-1 overflow-y-auto p-4">
      <Surface>
        <SurfaceHeader
          icon={<KeyRound aria-hidden />}
          action={
            <div className="flex items-center gap-2">
              {keys && keys.length > 6 && (
                <div className="relative w-40">
                  <Search aria-hidden className="pointer-events-none absolute top-2 left-2.5 size-4 text-muted-foreground" />
                  <Input
                    aria-label="Filter variables"
                    placeholder="Filter"
                    value={filter}
                    onChange={(e) => {
                      setInstant(true);
                      setFilter(e.target.value);
                    }}
                    onKeyDown={(e) => {
                      if (e.key === "Escape" && filter) {
                        e.stopPropagation();
                        setInstant(true);
                        setFilter("");
                      }
                    }}
                    className="bg-card pl-8"
                  />
                </div>
              )}
              {/* An empty environment shows these in its own empty state instead. */}
              {keys && keys.length > 0 && (
                <>
                  <Button variant="outline" onClick={openImport} className="bg-card">
                    <Upload aria-hidden />
                    Import
                  </Button>
                  <Button onClick={openAdd}>
                    <Plus aria-hidden />
                    Add variable
                  </Button>
                </>
              )}
            </div>
          }
        >
          <SurfaceTitle>{environment.name}</SurfaceTitle>
          <SurfaceDescription>
            {keys === null ? " " : `${keys.length} ${keys.length === 1 ? "variable" : "variables"}`}
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
                {visible.map((key) => (
                  <motion.li
                    key={key}
                    custom={instant}
                    variants={rowVariants}
                    initial={instant ? false : "hidden"}
                    animate="shown"
                    exit="hidden"
                    className="group flex min-h-11 items-center gap-3 overflow-hidden px-4 py-1.5"
                  >
                    <span className="selectable w-2/5 max-w-56 shrink-0 truncate font-mono text-[12.5px] font-medium" title={key}>
                      {key}
                    </span>
                    {/* Revealing "decrypts": the value sharpens in from a blur. */}
                    <span className="min-w-0 flex-1">
                      <AnimatePresence initial={false} mode="wait">
                        <motion.span
                          key={key in revealed ? "value" : "mask"}
                          initial={{ opacity: 0, filter: "blur(3px)" }}
                          animate={{ opacity: 1, filter: "blur(0px)" }}
                          exit={{ opacity: 0, filter: "blur(3px)" }}
                          transition={fast}
                          className={cn(
                            "block truncate font-mono text-[12.5px]",
                            key in revealed ? "selectable" : "tracking-widest text-muted-foreground",
                          )}
                          title={key in revealed ? revealed[key] : undefined}
                        >
                          {key in revealed
                            ? revealed[key] || <em className="font-sans text-muted-foreground">empty</em>
                            : MASK}
                        </motion.span>
                      </AnimatePresence>
                    </span>
                    <div className="flex shrink-0 items-center gap-0.5 opacity-0 transition-opacity group-focus-within:opacity-100 group-hover:opacity-100">
                      <IconButton label={key in revealed ? `Hide ${key}` : `Reveal ${key}`} onClick={() => toggleReveal(key)}>
                        <IconSwap active={key in revealed} on={<EyeOff aria-hidden />} off={<Eye aria-hidden />} />
                      </IconButton>
                      <IconButton label={`Copy ${key}`} onClick={() => copy(key)}>
                        <IconSwap active={copiedKey === key} on={<Check aria-hidden />} off={<Copy aria-hidden />} />
                      </IconButton>
                      <IconButton label={`Edit ${key}`} onClick={() => startEdit(key)}>
                        <Pencil aria-hidden />
                      </IconButton>
                      <IconButton label={`Delete ${key}`} onClick={() => setDeleting(key)} className="hover:text-destructive">
                        <Trash2 aria-hidden />
                      </IconButton>
                    </div>
                  </motion.li>
                ))}
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
      <ConfirmDialog
        open={deleting !== null}
        title={`Delete ${deleting}?`}
        description={`It will be removed from ${label} on every synced device.`}
        confirmLabel="Delete"
        onConfirm={deleteConfirmed}
        onCancel={() => setDeleting(null)}
      />
    </div>
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
