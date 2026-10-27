import type { Usage } from "@/lib/api";

/** "project / environment", with the environment standing out. */
export function EnvironmentName({ usage, muted = false }: { usage: Usage; muted?: boolean }) {
  return (
    <span className="min-w-0 truncate text-[13px]">
      <span className="text-muted-foreground">{usage.project} / </span>
      <span className={muted ? "" : "font-medium text-foreground"}>{usage.environment}</span>
    </span>
  );
}
