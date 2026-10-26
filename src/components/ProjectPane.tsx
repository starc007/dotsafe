import { useState } from "react";
import { toast } from "sonner";
import { Ellipsis, Plus, Trash2 } from "lucide-react";
import { api, message, type ProjectView } from "@/lib/api";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Tabs, TabsList, TabsTrigger } from "@/components/motion/tabs";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { DragRegion } from "@/components/DragRegion";
import { IconButton } from "@/components/IconButton";
import { InlineCreate } from "@/components/InlineCreate";
import { VariableList } from "@/components/VariableList";

type Pending = "environment" | "project" | null;

export function ProjectPane({ project, onChanged }: { project: ProjectView; onChanged: () => Promise<void> }) {
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
      if (confirm === "environment" && env) {
        await api.deleteEnvironment(env.id);
        toast(`Deleted ${env.name}`);
        setEnvId(null);
      } else if (confirm === "project") {
        await api.deleteProject(project.id);
        toast(`Deleted ${project.name}`);
      }
      await onChanged();
    } catch (e) {
      toast.error(message(e));
    } finally {
      setBusy(false);
      setConfirm(null);
    }
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <header data-tauri-drag-region className="flex h-12 shrink-0 items-center gap-3 border-b px-4">
        <h1 data-tauri-drag-region className="truncate text-sm font-semibold">
          {project.name}
        </h1>
        <div className="flex min-w-0 items-center gap-1">
          {project.environments.length > 0 && (
            <Tabs value={env?.id ?? ""} onValueChange={setEnvId} className="min-w-0">
              <TabsList aria-label="Environments" className="bg-muted p-0.5">
                {project.environments.map((e) => (
                  <TabsTrigger key={e.id} value={e.id} className="px-3 py-1 text-[13px]">
                    {e.name}
                  </TabsTrigger>
                ))}
              </TabsList>
            </Tabs>
          )}
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
              <Plus aria-hidden />
            </IconButton>
          )}
        </div>
        <div data-tauri-drag-region className="flex-1 self-stretch" />
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant="ghost" size="icon-sm" aria-label="More actions">
              <Ellipsis aria-hidden />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end" className="w-auto min-w-44 *:whitespace-nowrap">
            {env && (
              <DropdownMenuItem variant="destructive" onSelect={() => setConfirm("environment")}>
                <Trash2 aria-hidden />
                Delete {env.name}…
              </DropdownMenuItem>
            )}
            <DropdownMenuItem variant="destructive" onSelect={() => setConfirm("project")}>
              <Trash2 aria-hidden />
              Delete project…
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </header>

      {env ? (
        <VariableList key={env.id} environment={env} projectName={project.name} onChanged={onChanged} />
      ) : (
        <div className="relative flex flex-1 flex-col items-center justify-center gap-2 text-center">
          <DragRegion className="absolute inset-x-0 top-0 h-12" />
          <p className="text-sm font-semibold">No environments</p>
          <p className="text-muted-foreground">Add one with the + button above.</p>
        </div>
      )}

      <ConfirmDialog
        open={confirm !== null}
        busy={busy}
        title={confirm === "project" ? `Delete ${project.name}?` : `Delete ${env?.name}?`}
        description={
          confirm === "project"
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
