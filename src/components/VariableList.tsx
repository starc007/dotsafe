import { useCallback, useEffect, useRef, useState, type FormEvent } from "react";
import { toast } from "sonner";
import { Copy, Eye, EyeOff, KeyRound, Pencil, Plus, Search, Trash2 } from "lucide-react";
import { api, message, type EnvironmentView } from "@/lib/api";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Kbd } from "@/components/ui/kbd";
import { Skeleton } from "@/components/ui/skeleton";
import { Spinner } from "@/components/ui/spinner";
import { Textarea } from "@/components/ui/textarea";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { IconButton } from "@/components/IconButton";

/** Revealed values hide again after this long. */
const REVEAL_MS = 30_000;
const MASK = "••••••••••••";
/** Shared by the list rows and the edit row so the columns line up. */
const NAME_COLUMN = "w-2/5 max-w-56 shrink-0";

type Editing = { mode: "new" } | { mode: "edit"; key: string; value: string } | null;

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
  const [keys, setKeys] = useState<string[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [revealed, setRevealed] = useState<Record<string, string>>({});
  const [editing, setEditing] = useState<Editing>(null);
  const [deleting, setDeleting] = useState<string | null>(null);
  const [filter, setFilter] = useState("");
  const timers = useRef(new Map<string, number>());

  const load = useCallback(async () => {
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
      toast(`Copied ${key}`, { description: "The clipboard clears in 30 seconds." });
    } catch (e) {
      toast.error(message(e));
    }
  }

  async function startEdit(key: string) {
    try {
      const value = revealed[key] ?? (await api.revealVariable(envId, key));
      setEditing({ mode: "edit", key, value });
    } catch (e) {
      toast.error(message(e));
    }
  }

  async function saved(key: string, previousKey?: string) {
    setEditing(null);
    if (previousKey) hide(previousKey);
    hide(key);
    await load();
    await onChanged();
    toast.success(previousKey ? `Saved ${key}` : `Added ${key}`);
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

  const visible = keys?.filter((k) => k.toLowerCase().includes(filter.trim().toLowerCase())) ?? [];

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex items-center gap-2 px-4 py-3">
        <p className="text-xs text-muted-foreground">
          {keys === null ? " " : `${keys.length} ${keys.length === 1 ? "variable" : "variables"}`}
        </p>
        <div className="flex-1" />
        {keys && keys.length > 6 && (
          <div className="relative w-48">
            <Search aria-hidden className="pointer-events-none absolute top-2 left-2 size-4 text-muted-foreground" />
            <Input
              aria-label="Filter variables"
              placeholder="Filter"
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
              className="pl-7"
            />
          </div>
        )}
        <Button onClick={() => setEditing({ mode: "new" })} disabled={editing?.mode === "new"}>
          <Plus aria-hidden />
          Add variable
        </Button>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto px-4 pb-6">
        {editing?.mode === "new" && (
          <EditRow envId={envId} onSaved={(k) => saved(k)} onCancel={() => setEditing(null)} />
        )}

        {keys === null ? (
          <RowsSkeleton />
        ) : keys.length === 0 && editing?.mode !== "new" ? (
          <div className="flex flex-col items-center gap-2 py-16 text-center">
            <div className="mb-1 rounded-xl bg-muted p-3 text-muted-foreground">
              <KeyRound aria-hidden className="size-5" />
            </div>
            <p className="text-sm font-semibold">
              No variables in {projectName}/{environment.name}
            </p>
            <p className="max-w-sm text-muted-foreground">
              Add one here, or import an existing file from the terminal with{" "}
              <code className="selectable rounded bg-muted px-1 py-0.5 font-mono text-xs">dotsafe import</code>.
            </p>
          </div>
        ) : (
          <ul aria-label="Variables" className="divide-y rounded-lg border">
            {visible.map((key) =>
              editing?.mode === "edit" && editing.key === key ? (
                <li key={key}>
                  <EditRow
                    envId={envId}
                    initialKey={key}
                    initialValue={editing.value}
                    onSaved={(k) => saved(k, key)}
                    onCancel={() => setEditing(null)}
                    bare
                  />
                </li>
              ) : (
                <li key={key} className="group flex min-h-10 items-center gap-3 px-3 py-1.5">
                  <span className={cn(NAME_COLUMN, "selectable truncate font-mono text-[12.5px] font-medium")} title={key}>
                    {key}
                  </span>
                  <span
                    className={cn(
                      "min-w-0 flex-1 truncate font-mono text-[12.5px]",
                      key in revealed ? "selectable" : "tracking-widest text-muted-foreground",
                    )}
                    title={key in revealed ? revealed[key] : undefined}
                  >
                    {key in revealed ? revealed[key] || <em className="font-sans text-muted-foreground">empty</em> : MASK}
                  </span>
                  <div className="flex shrink-0 items-center gap-0.5 opacity-0 transition-opacity group-focus-within:opacity-100 group-hover:opacity-100">
                    <IconButton label={key in revealed ? `Hide ${key}` : `Reveal ${key}`} onClick={() => toggleReveal(key)}>
                      {key in revealed ? <EyeOff aria-hidden /> : <Eye aria-hidden />}
                    </IconButton>
                    <IconButton label={`Copy ${key}`} onClick={() => copy(key)}>
                      <Copy aria-hidden />
                    </IconButton>
                    <IconButton label={`Edit ${key}`} onClick={() => startEdit(key)}>
                      <Pencil aria-hidden />
                    </IconButton>
                    <IconButton label={`Delete ${key}`} onClick={() => setDeleting(key)} className="hover:text-destructive">
                      <Trash2 aria-hidden />
                    </IconButton>
                  </div>
                </li>
              ),
            )}
            {visible.length === 0 && keys.length > 0 && (
              <li className="px-3 py-6 text-center text-muted-foreground">No variables match “{filter}”.</li>
            )}
          </ul>
        )}
      </div>

      <ConfirmDialog
        open={deleting !== null}
        title={`Delete ${deleting}?`}
        description={`It will be removed from ${projectName}/${environment.name} on every synced device.`}
        confirmLabel="Delete"
        onConfirm={deleteConfirmed}
        onCancel={() => setDeleting(null)}
      />
    </div>
  );
}

function EditRow({
  envId,
  initialKey = "",
  initialValue = "",
  onSaved,
  onCancel,
  bare,
}: {
  envId: string;
  initialKey?: string;
  initialValue?: string;
  onSaved: (key: string) => void;
  onCancel: () => void;
  /** Inside the list (no own border), when editing an existing row. */
  bare?: boolean;
}) {
  const [key, setKey] = useState(initialKey);
  const [value, setValue] = useState(initialValue);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function submit(e?: FormEvent) {
    e?.preventDefault();
    setBusy(true);
    setError(null);
    try {
      await api.setVariable(envId, key, value, initialKey || undefined);
      onSaved(key.trim());
    } catch (err) {
      setError(message(err));
      setBusy(false);
    }
  }

  function onKeyDown(e: React.KeyboardEvent) {
    if (e.key === "Escape") onCancel();
    // Enter saves; Shift+Enter adds a line (certificates, private keys).
    if (e.key === "Enter" && !e.shiftKey && e.currentTarget.tagName === "TEXTAREA") {
      e.preventDefault();
      submit();
    }
  }

  return (
    <form
      onSubmit={submit}
      className={cn("flex flex-col gap-2 bg-muted/40 p-3", !bare && "mb-3 rounded-lg border")}
    >
      <div className="flex items-start gap-3">
        <div className={NAME_COLUMN}>
          <Input
            aria-label="Name"
            placeholder="API_KEY"
            autoFocus={!initialKey}
            autoCorrect="off"
            autoCapitalize="off"
            spellCheck={false}
            value={key}
            aria-invalid={!!error}
            onKeyDown={onKeyDown}
            onChange={(e) => setKey(e.target.value.replace(/\s/g, "_"))}
            className="bg-background font-mono text-[12.5px] md:text-[12.5px]"
          />
        </div>
        <Textarea
          aria-label="Value"
          placeholder="Value"
          autoFocus={!!initialKey}
          autoCorrect="off"
          autoCapitalize="off"
          spellCheck={false}
          value={value}
          onKeyDown={onKeyDown}
          onChange={(e) => setValue(e.target.value)}
          className="max-h-48 min-h-8 flex-1 resize-none bg-background py-1.5 font-mono text-[12.5px] md:text-[12.5px]"
        />
      </div>
      <div className="flex items-center gap-2">
        {error ? (
          <p role="alert" className="text-xs text-destructive">
            {error}
          </p>
        ) : (
          <p className="flex items-center gap-1 text-xs text-muted-foreground">
            <Kbd>Enter</Kbd> save <span aria-hidden>·</span> <Kbd>Shift</Kbd>
            <Kbd>Enter</Kbd> new line <span aria-hidden>·</span> <Kbd>Esc</Kbd> cancel
          </p>
        )}
        <div className="flex-1" />
        <Button type="button" variant="ghost" onClick={onCancel}>
          Cancel
        </Button>
        <Button type="submit" disabled={busy || !key.trim()}>
          {busy && <Spinner />}
          Save
        </Button>
      </div>
    </form>
  );
}

function RowsSkeleton() {
  return (
    <div aria-hidden className="divide-y rounded-lg border">
      {[0, 1, 2].map((i) => (
        <div key={i} className="flex h-10 items-center gap-3 px-3">
          <Skeleton className="h-3.5 w-40" />
          <Skeleton className="h-3.5 w-28" />
        </div>
      ))}
    </div>
  );
}
