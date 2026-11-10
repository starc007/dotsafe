import { useState } from "react";
import { toast } from "sonner";
import { CornerDownRight, Ellipsis, GitCompareArrows, Plus, Trash2 } from "lucide-react";
import { api, message, type EnvironmentView, type ProjectView } from "@/lib/api";
import { SHORTCUTS, ariaShortcut, useShortcut } from "@/lib/shortcuts";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Tabs, TabsList, TabsTrigger } from "@/components/motion/tabs";
import { CompareDialog, defaultOther } from "@/components/CompareDialog";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { ShortcutKeys } from "@/components/ShortcutKeys";
import { VariableDialog } from "@/components/VariableDialog";
import { DragRegion } from "@/components/DragRegion";
import { IconButton } from "@/components/IconButton";
import { InlineCreate } from "@/components/InlineCreate";
import { VariableList } from "@/components/VariableList";

type Pending = "environment" | "project" | null;

export function ProjectPane({
  project,
  initialEnvironmentId,
  onChanged,
}: {
  project: ProjectView;
  /** Opens on this environment instead of the first. */
  initialEnvironmentId?: string | null;
  onChanged: () => Promise<void>;
}) {
  const [envId, setEnvId] = useState<string | null>(initialEnvironmentId ?? project.environments[0]?.id ?? null);
  const [creating, setCreating] = useState(false);
  const [confirm, setConfirm] = useState<Pending>(null);
  const [busy, setBusy] = useState(false);
  const [compareWith, setCompareWith] = useState<string | null>(null);
  /** A variable being added from the compare dialog, which hides meanwhile. */
  const [adding, setAdding] = useState<{ environment: EnvironmentView; key: string } | null>(null);
  /** Bumped to reload the variable list after the compare dialog adds one. */
  const [listVersion, setListVersion] = useState(0);
  const env = project.environments.find((e) => e.id === envId) ?? project.environments[0] ?? null;
  const canCompare = !!env && project.environments.length > 1;

  function openCompare() {
    if (!env || !canCompare) return;
    setCompareWith(defaultOther(project.environments, env)?.id ?? null);
  }
  useShortcut(SHORTCUTS.compareEnvironments, openCompare, canCompare);

  async function addedFromCompare() {
    setAdding(null);
    setListVersion((v) => v + 1);
    await onChanged();
  }

  function stepEnvironment(delta: number) {
    const envs = project.environments;
    if (envs.length < 2 || !env) return;
    const at = envs.findIndex((e) => e.id === env.id);
    setEnvId(envs[(at + delta + envs.length) % envs.length].id);
  }
  useShortcut(SHORTCUTS.previousEnvironment, () => stepEnvironment(-1));
  useShortcut(SHORTCUTS.nextEnvironment, () => stepEnvironment(1));

  async function createEnvironment(name: string) {
    const created = await api.createEnvironment(project.id, name);
    await onChanged();
    setEnvId(created.id);
    setCreating(false);
  }

  async function setParent(parent: string | null) {
    if (!env) return;
    try {
      await api.setEnvironmentParent(env.id, parent);
      const name = project.environments.find((e) => e.id === parent)?.name;
      toast(name ? `${env.name} inherits from ${name}` : `${env.name} doesn't inherit anymore`);
      await onChanged();
    } catch (e) {
      toast.error(message(e));
    }
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
            {env && canCompare && (
              <>
                <DropdownMenuItem onSelect={openCompare} aria-keyshortcuts={ariaShortcut(SHORTCUTS.compareEnvironments)}>
                  <GitCompareArrows aria-hidden />
                  Compare {env.name}
                  <ShortcutKeys shortcut={SHORTCUTS.compareEnvironments} className="ml-auto pl-4" />
                </DropdownMenuItem>
                <DropdownMenuSub>
                  <DropdownMenuSubTrigger>
                    <CornerDownRight aria-hidden />
                    {env.name} inherits from
                  </DropdownMenuSubTrigger>
                  <DropdownMenuSubContent>
                    <DropdownMenuRadioGroup value={env.parent ?? ""} onValueChange={(v) => setParent(v || null)}>
                      <DropdownMenuRadioItem value="">Nothing</DropdownMenuRadioItem>
                      {project.environments
                        .filter((e) => e.id !== env.id)
                        .map((e) => (
                          <DropdownMenuRadioItem key={e.id} value={e.id}>
                            {e.name}
                          </DropdownMenuRadioItem>
                        ))}
                    </DropdownMenuRadioGroup>
                  </DropdownMenuSubContent>
                </DropdownMenuSub>
                <DropdownMenuSeparator />
              </>
            )}
            {env && (
              <DropdownMenuItem variant="destructive" onSelect={() => setConfirm("environment")}>
                <Trash2 aria-hidden />
                Delete {env.name}
              </DropdownMenuItem>
            )}
            <DropdownMenuItem variant="destructive" onSelect={() => setConfirm("project")}>
              <Trash2 aria-hidden />
              Delete project
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </header>

      {env ? (
        // Remounts when the parent changes, so inherited rows reload.
        <VariableList key={`${env.id}:${env.parent ?? ""}:${listVersion}`} environment={env} projectName={project.name} onChanged={onChanged} />
      ) : (
        <div className="relative flex flex-1 flex-col items-center justify-center gap-2 text-center">
          <DragRegion className="absolute inset-x-0 top-0 h-12" />
          <p className="text-sm font-semibold">No environments</p>
          <p className="text-muted-foreground">Add one with the + button above.</p>
        </div>
      )}

      {env && canCompare && compareWith && (
        <CompareDialog
          open={!adding}
          onOpenChange={(open) => !open && setCompareWith(null)}
          environment={env}
          environments={project.environments}
          otherId={compareWith}
          onOtherChange={setCompareWith}
          onAdd={(environment, key) => setAdding({ environment, key })}
        />
      )}
      {adding && (
        <VariableDialog
          open
          onOpenChange={(open) => !open && setAdding(null)}
          environmentId={adding.environment.id}
          environmentLabel={`${project.name}/${adding.environment.name}`}
          defaultKey={adding.key}
          onSaved={(key) => {
            toast(`Added ${key} to ${adding.environment.name}`);
            void addedFromCompare();
          }}
          onImported={() => void addedFromCompare()}
        />
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
