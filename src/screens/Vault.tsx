import { useCallback, useEffect, useState } from "react";
import { FolderLock, Lock, Plus } from "lucide-react";
import { api, message, type ProjectView } from "../lib/api";
import { Button, DragRegion, IconButton } from "../components/ui";
import { InlineCreate } from "../components/InlineCreate";
import { ProjectPane } from "../components/ProjectPane";
import { Toast, useToast } from "../components/Toast";

export function VaultScreen({ onLocked }: { onLocked: () => void }) {
  const [projects, setProjects] = useState<ProjectView[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const toast = useToast();

  const load = useCallback(async () => {
    try {
      const list = await api.listProjects();
      setProjects(list);
      setError(null);
      setSelectedId((id) => (id && list.some((p) => p.id === id) ? id : (list[0]?.id ?? null)));
    } catch (e) {
      setError(message(e));
    }
  }, []);

  useEffect(() => {
    load();
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

  const selected = projects?.find((p) => p.id === selectedId) ?? null;

  return (
    <div className="flex h-full">
      <aside className="flex w-56 shrink-0 flex-col border-r border-border bg-sidebar">
        {/* Space for the macOS traffic lights; also drags the window. */}
        <DragRegion className="h-12 shrink-0" />
        <div className="flex items-center justify-between px-3 pb-1">
          <h2 className="px-1 text-xs font-medium text-muted-foreground">Projects</h2>
          <IconButton label="New project" onClick={() => setCreating(true)}>
            <Plus aria-hidden className="size-4" />
          </IconButton>
        </div>
        <nav aria-label="Projects" className="flex-1 overflow-y-auto px-2 pb-2">
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
          {projects?.map((p) => (
            <button
              key={p.id}
              aria-current={p.id === selectedId ? "page" : undefined}
              onClick={() => setSelectedId(p.id)}
              className="flex h-8 w-full items-center gap-2 rounded-md px-2 text-left text-[13px] text-foreground/90 hover:bg-sidebar-accent aria-[current=page]:bg-sidebar-accent aria-[current=page]:font-medium focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            >
              <span className="truncate">{p.name}</span>
            </button>
          ))}
        </nav>
        <div className="border-t border-border p-2">
          <Button variant="ghost" className="w-full justify-start text-muted-foreground hover:bg-sidebar-accent" onClick={lock}>
            <Lock aria-hidden className="size-3.5" />
            Lock
          </Button>
        </div>
      </aside>

      <section className="flex min-w-0 flex-1 flex-col">
        {error ? (
          <Message title="Couldn't load your projects" body={error}>
            <Button onClick={load}>Try again</Button>
          </Message>
        ) : projects === null ? (
          <DragRegion className="h-12" />
        ) : selected ? (
          <ProjectPane key={selected.id} project={selected} onChanged={load} notify={toast.show} />
        ) : (
          <Message
            icon={<FolderLock aria-hidden className="size-5" />}
            title="No projects yet"
            body="A project holds the variables for one app, split into environments like development and production."
          >
            <Button variant="primary" onClick={() => setCreating(true)}>
              <Plus aria-hidden className="size-3.5" />
              New project
            </Button>
          </Message>
        )}
      </section>

      <Toast message={toast.message} />
    </div>
  );
}

function Message({
  icon,
  title,
  body,
  children,
}: {
  icon?: React.ReactNode;
  title: string;
  body: string;
  children?: React.ReactNode;
}) {
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
    <div aria-hidden className="flex flex-col gap-1.5 px-2 pt-1">
      {[70, 55, 80].map((w) => (
        <div key={w} className="h-5 animate-pulse rounded bg-muted" style={{ width: `${w}%` }} />
      ))}
    </div>
  );
}
