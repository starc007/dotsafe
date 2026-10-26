import { useEffect, useRef, useState } from "react";
import { Ellipsis, Plus } from "lucide-react";
import { api, type ProjectView } from "../lib/api";
import { ConfirmDialog, DragRegion, IconButton } from "./ui";
import { InlineCreate } from "./InlineCreate";
import { VariableList } from "./VariableList";

type Pending = { kind: "environment" } | { kind: "project" } | null;

export function ProjectPane({
  project,
  onChanged,
  notify,
}: {
  project: ProjectView;
  onChanged: () => Promise<void>;
  notify: (text: string) => void;
}) {
  const [envId, setEnvId] = useState<string | null>(project.environments[0]?.id ?? null);
  const [creating, setCreating] = useState(false);
  const [confirm, setConfirm] = useState<Pending>(null);
  const [busy, setBusy] = useState(false);
  const env = project.environments.find((e) => e.id === envId) ?? project.environments[0] ?? null;

  async function createEnvironment(name: string) {
    const created = await api.createEnvironment(project.id, name);
    await onChanged();
    setEnvId(created.id);
    setCreating(false);
  }

  async function deleteConfirmed() {
    setBusy(true);
    try {
      if (confirm?.kind === "environment" && env) {
        await api.deleteEnvironment(env.id);
        notify(`Deleted ${env.name}`);
        setEnvId(null);
      } else if (confirm?.kind === "project") {
        await api.deleteProject(project.id);
        notify(`Deleted ${project.name}`);
      }
      await onChanged();
    } finally {
      setBusy(false);
      setConfirm(null);
    }
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <header data-tauri-drag-region className="flex h-12 shrink-0 items-center gap-3 border-b border-border px-4">
        <h1 data-tauri-drag-region className="truncate text-sm font-semibold">
          {project.name}
        </h1>
        <div role="tablist" aria-label="Environments" className="flex min-w-0 items-center gap-0.5 overflow-x-auto">
          {project.environments.map((e) => (
            <button
              key={e.id}
              role="tab"
              aria-selected={e.id === env?.id}
              onClick={() => setEnvId(e.id)}
              className="h-7 shrink-0 rounded-md px-2.5 text-[13px] text-muted-foreground hover:bg-muted hover:text-foreground aria-selected:bg-muted aria-selected:font-medium aria-selected:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            >
              {e.name}
            </button>
          ))}
          {creating ? (
            <InlineCreate
              label="Environment name"
              placeholder="staging"
              className="w-32"
              onCreate={createEnvironment}
              onCancel={() => setCreating(false)}
            />
          ) : (
            <IconButton label="New environment" onClick={() => setCreating(true)}>
              <Plus aria-hidden className="size-4" />
            </IconButton>
          )}
        </div>
        <div data-tauri-drag-region className="flex-1 self-stretch" />
        <Menu
          items={[
            ...(env ? [{ label: `Delete ${env.name}…`, onSelect: () => setConfirm({ kind: "environment" }) }] : []),
            { label: "Delete project…", onSelect: () => setConfirm({ kind: "project" }) },
          ]}
        />
      </header>

      {env ? (
        <VariableList key={env.id} environment={env} projectName={project.name} notify={notify} onChanged={onChanged} />
      ) : (
        <div className="flex flex-1 flex-col items-center justify-center gap-2 text-center">
          <DragRegion className="absolute inset-x-0 top-0 h-12" />
          <p className="text-sm font-semibold">No environments</p>
          <p className="text-muted-foreground">Add one with the + button above.</p>
        </div>
      )}

      <ConfirmDialog
        open={confirm !== null}
        busy={busy}
        title={confirm?.kind === "project" ? `Delete ${project.name}?` : `Delete ${env?.name}?`}
        body={
          confirm?.kind === "project"
            ? "All of its environments and variables will be deleted on every device synced to this vault."
            : "Its variables will be deleted on every device synced to this vault."
        }
        confirmLabel="Delete"
        onConfirm={deleteConfirmed}
        onCancel={() => setConfirm(null)}
      />
    </div>
  );
}

/** A small dropdown menu: arrow keys move, Escape or a click outside closes. */
function Menu({ items }: { items: { label: string; onSelect: () => void }[] }) {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => !root.current?.contains(e.target as Node) && setOpen(false);
    document.addEventListener("mousedown", onDown);
    root.current?.querySelector<HTMLButtonElement>("[role=menuitem]")?.focus();
    return () => document.removeEventListener("mousedown", onDown);
  }, [open]);

  function onKeyDown(e: React.KeyboardEvent) {
    const buttons = [...(root.current?.querySelectorAll<HTMLButtonElement>("[role=menuitem]") ?? [])];
    const i = buttons.indexOf(document.activeElement as HTMLButtonElement);
    if (e.key === "Escape") {
      setOpen(false);
      trigger.current?.focus();
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      buttons[(i + 1) % buttons.length]?.focus();
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      buttons[(i - 1 + buttons.length) % buttons.length]?.focus();
    }
  }

  return (
    <div ref={root} className="relative" onKeyDown={onKeyDown}>
      <IconButton
        ref={trigger}
        label="More actions"
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen((o) => !o)}
      >
        <Ellipsis aria-hidden className="size-4" />
      </IconButton>
      {open && (
        <div
          role="menu"
          className="animate-in absolute right-0 top-8 z-10 min-w-44 rounded-lg border border-border bg-background p-1 shadow-lg"
        >
          {items.map((item) => (
            <button
              key={item.label}
              role="menuitem"
              onClick={() => {
                setOpen(false);
                item.onSelect();
              }}
              className="flex h-7 w-full items-center rounded-md px-2 text-left text-[13px] text-danger hover:bg-danger-muted focus:bg-danger-muted focus:outline-none"
            >
              {item.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
