import { useEffect, useMemo, useState } from "react";
import { toast } from "sonner";
import { EyeOff, FolderOpen, KeyRound, Variable } from "lucide-react";
import { api, message, type ProjectView, type VariableHit } from "@/lib/api";
import { CommandPalette, type CommandItem } from "@/components/motion/command-palette";
import { EnvironmentName } from "@/components/EnvironmentName";

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
      mono: true,
      group: "Variables",
      keywords: [v.project, v.environment],
      // Linked variables take their value from a secret.
      icon: v.linked ? KeyRound : Variable,
      action: "Copy",
      badge: <EnvironmentName usage={v} muted />,
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
        group: "Environments",
        icon: FolderOpen,
        action: "Open",
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
      footer={
        <>
          <EyeOff aria-hidden className="size-3.5 shrink-0" />
          Values are copied, never shown
        </>
      }
    />
  );
}
