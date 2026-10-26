import { useCallback, useEffect, useState, type ReactNode } from "react";
import { FolderLock, Lock, Plus } from "lucide-react";
import { api, message, type ProjectView } from "@/lib/api";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { DragRegion } from "@/components/DragRegion";
import { IconButton } from "@/components/IconButton";
import { InlineCreate } from "@/components/InlineCreate";
import { ProjectPane } from "@/components/ProjectPane";

export function VaultScreen({ onLocked }: { onLocked: () => void }) {
  const [projects, setProjects] = useState<ProjectView[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);

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
      <aside className="flex w-56 shrink-0 flex-col border-r border-sidebar-border bg-sidebar text-sidebar-foreground">
        {/* Space for the macOS traffic lights; also drags the window. */}
        <DragRegion className="h-12 shrink-0" />
        <div className="flex items-center justify-between px-3 pb-1">
          <h2 className="px-1 text-xs font-medium text-muted-foreground">Projects</h2>
          <IconButton label="New project" onClick={() => setCreating(true)}>
            <Plus aria-hidden />
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
              className={cn(
                "flex h-8 w-full items-center rounded-md px-2 text-left outline-none transition-colors",
                "hover:bg-sidebar-accent focus-visible:ring-2 focus-visible:ring-sidebar-ring",
                "aria-[current=page]:bg-sidebar-accent aria-[current=page]:font-medium aria-[current=page]:text-sidebar-accent-foreground",
              )}
            >
              <span className="truncate">{p.name}</span>
            </button>
          ))}
        </nav>
        <div className="border-t border-sidebar-border p-2">
          <Button variant="ghost" className="w-full justify-start text-muted-foreground hover:bg-sidebar-accent" onClick={lock}>
            <Lock aria-hidden />
            Lock
          </Button>
        </div>
      </aside>

      <section className="flex min-w-0 flex-1 flex-col">
        {error ? (
          <Message title="Couldn't load your projects" body={error}>
            <Button variant="outline" onClick={load}>
              Try again
            </Button>
          </Message>
        ) : projects === null ? (
          <DragRegion className="h-12" />
        ) : selected ? (
          <ProjectPane key={selected.id} project={selected} onChanged={load} />
        ) : (
          <Message
            icon={<FolderLock aria-hidden className="size-5" />}
            title="No projects yet"
            body="A project holds the variables for one app, split into environments like development and production."
          >
            <Button onClick={() => setCreating(true)}>
              <Plus aria-hidden />
              New project
            </Button>
          </Message>
        )}
      </section>
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
