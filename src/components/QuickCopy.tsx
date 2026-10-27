import { useEffect, useMemo, useState } from "react";
import { toast } from "sonner";
import { FolderOpen, KeyRound, Link2 } from "lucide-react";
import { api, message, type ProjectView, type VariableHit } from "@/lib/api";
import { CommandPalette, type CommandItem } from "@/components/motion/command-palette";

/** ⌘K: find any variable in any project and copy it, or jump to an
 * environment. Only names reach the webview; copying happens in Rust. */
export function QuickCopy({
  open,
  onOpenChange,
  projects,
  onOpenEnvironment,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  projects: ProjectView[];
  onOpenEnvironment: (projectId: string, environmentId: string) => void;
}) {
  const [variables, setVariables] = useState<VariableHit[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return;
    setError(null);
    api
      .allVariables()
      .then(setVariables)
      .catch((e) => setError(message(e)));
  }, [open]);

  const items = useMemo<CommandItem[]>(() => {
    const vars: CommandItem[] = (variables ?? []).map((v) => ({
      id: `var:${v.environmentId}:${v.key}`,
      label: v.key,
      group: "Copy a variable",
      keywords: [v.project, v.environment],
      icon: v.linked ? Link2 : KeyRound,
      badge: (
        <span className="text-xs text-muted-foreground">
          {v.project} / {v.environment}
        </span>
      ),
      onSelect: async () => {
        try {
          await api.copyVariable(v.environmentId, v.key);
          toast(`Copied ${v.key}`, { description: `From ${v.project} / ${v.environment}. The clipboard clears in 30 seconds.` });
        } catch (e) {
          toast.error(message(e));
        }
      },
    }));
    const envs: CommandItem[] = projects.flatMap((p) =>
      p.environments.map((e) => ({
        id: `env:${e.id}`,
        label: `${p.name} / ${e.name}`,
        group: "Go to",
        icon: FolderOpen,
        onSelect: () => onOpenEnvironment(p.id, e.id),
      })),
    );
    return [...vars, ...envs];
  }, [variables, projects, onOpenEnvironment]);

  return (
    <CommandPalette
      open={open}
      onOpenChange={onOpenChange}
      shortcut={null}
      items={items}
      placeholder="Find a variable…"
      emptyMessage={error ?? (variables === null ? "Loading variables…" : "No matching variables.")}
      footer="Enter copies the value. It never appears on screen."
    />
  );
}
