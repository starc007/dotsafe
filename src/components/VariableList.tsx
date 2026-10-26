import { useCallback, useEffect, useRef, useState, type FormEvent } from "react";
import { Copy, Eye, EyeOff, KeyRound, Pencil, Plus, Search, Trash2 } from "lucide-react";
import { api, message, type EnvironmentView } from "../lib/api";
import { Button, ConfirmDialog, IconButton, Input } from "./ui";

/** Revealed values hide again after this long. */
const REVEAL_MS = 30_000;
const MASK = "••••••••••••";

type Editing = { mode: "new" } | { mode: "edit"; key: string; value: string } | null;

export function VariableList({
  environment,
  projectName,
  notify,
  onChanged,
}: {
  environment: EnvironmentView;
  projectName: string;
  notify: (text: string) => void;
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
      notify(message(e));
    }
  }

  async function copy(key: string) {
    try {
      await api.copyVariable(envId, key);
      notify(`Copied ${key}. Clipboard clears in 30 seconds.`);
    } catch (e) {
      notify(message(e));
    }
  }

  async function startEdit(key: string) {
    try {
      const value = revealed[key] ?? (await api.revealVariable(envId, key));
      setEditing({ mode: "edit", key, value });
    } catch (e) {
      notify(message(e));
    }
  }

  async function saved(key: string, previousKey?: string) {
    setEditing(null);
    if (previousKey) hide(previousKey);
    hide(key);
    await load();
    await onChanged();
    notify(previousKey ? `Saved ${key}` : `Added ${key}`);
  }

  async function deleteConfirmed() {
    if (!deleting) return;
    try {
      await api.deleteVariable(envId, deleting);
      hide(deleting);
      notify(`Deleted ${deleting}`);
      await load();
      await onChanged();
    } catch (e) {
      notify(message(e));
    } finally {
      setDeleting(null);
    }
  }

  if (error) {
    return (
      <div className="flex flex-1 flex-col items-center justify-center gap-3 text-center">
        <p className="text-sm font-semibold">Couldn't load variables</p>
        <p className="selectable max-w-sm text-muted-foreground">{error}</p>
        <Button onClick={load}>Try again</Button>
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
            <Search aria-hidden className="pointer-events-none absolute left-2 top-2 size-4 text-muted-foreground" />
            <Input
              aria-label="Filter variables"
              placeholder="Filter"
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
              className="h-8 pl-7"
            />
          </div>
        )}
        <Button variant="primary" onClick={() => setEditing({ mode: "new" })} disabled={editing?.mode === "new"}>
          <Plus aria-hidden className="size-3.5" />
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
          <ul aria-label="Variables" className="divide-y divide-border rounded-lg border border-border">
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
                  <span className="selectable w-2/5 max-w-56 shrink-0 truncate font-mono text-[12.5px] font-medium" title={key}>
                    {key}
                  </span>
                  <span
                    className={`min-w-0 flex-1 truncate font-mono text-[12.5px] ${key in revealed ? "selectable" : "tracking-widest text-muted-foreground"}`}
                    title={key in revealed ? revealed[key] : undefined}
                  >
                    {key in revealed ? revealed[key] || <em className="font-sans text-muted-foreground">empty</em> : MASK}
                  </span>
                  <div className="flex shrink-0 items-center gap-0.5 opacity-0 transition-opacity group-hover:opacity-100 group-focus-within:opacity-100">
                    <IconButton label={key in revealed ? `Hide ${key}` : `Reveal ${key}`} onClick={() => toggleReveal(key)}>
                      {key in revealed ? <EyeOff aria-hidden className="size-4" /> : <Eye aria-hidden className="size-4" />}
                    </IconButton>
                    <IconButton label={`Copy ${key}`} onClick={() => copy(key)}>
                      <Copy aria-hidden className="size-4" />
                    </IconButton>
                    <IconButton label={`Edit ${key}`} onClick={() => startEdit(key)}>
                      <Pencil aria-hidden className="size-4" />
                    </IconButton>
                    <IconButton label={`Delete ${key}`} onClick={() => setDeleting(key)} className="hover:text-danger">
                      <Trash2 aria-hidden className="size-4" />
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
        body={`It will be removed from ${projectName}/${environment.name} on every synced device.`}
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
  const multiline = value.includes("\n");

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
      onKeyDown={onKeyDown}
      className={`flex flex-col gap-2 p-3 ${bare ? "bg-surface" : "mb-3 rounded-lg border border-border bg-surface"}`}
    >
      <div className="flex items-start gap-3">
        <div className="w-2/5 max-w-56 shrink-0">
          <Input
            aria-label="Name"
            placeholder="API_KEY"
            autoFocus={!initialKey}
            value={key}
            aria-invalid={!!error}
            onChange={(e) => setKey(e.target.value.replace(/\s/g, "_"))}
            className="font-mono text-[12.5px]"
          />
        </div>
        <textarea
          aria-label="Value"
          placeholder="Value"
          autoFocus={!!initialKey}
          rows={multiline ? Math.min(value.split("\n").length, 8) : 1}
          spellCheck={false}
          autoCorrect="off"
          autoCapitalize="off"
          value={value}
          onChange={(e) => setValue(e.target.value)}
          className="min-h-8 flex-1 resize-none rounded-md border border-input bg-background px-2.5 py-[7px] font-mono text-[12.5px] leading-4 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        />
      </div>
      <div className="flex items-center gap-2">
        {error ? (
          <p role="alert" className="text-xs text-danger">
            {error}
          </p>
        ) : (
          <p className="text-xs text-muted-foreground">Enter to save · Shift+Enter for a new line · Esc to cancel</p>
        )}
        <div className="flex-1" />
        <Button type="button" variant="ghost" onClick={onCancel}>
          Cancel
        </Button>
        <Button type="submit" variant="primary" loading={busy} disabled={!key.trim()}>
          Save
        </Button>
      </div>
    </form>
  );
}

function RowsSkeleton() {
  return (
    <div aria-hidden className="divide-y divide-border rounded-lg border border-border">
      {[0, 1, 2].map((i) => (
        <div key={i} className="flex h-10 items-center gap-3 px-3">
          <div className="h-3.5 w-40 animate-pulse rounded bg-muted" />
          <div className="h-3.5 w-28 animate-pulse rounded bg-muted" />
        </div>
      ))}
    </div>
  );
}
