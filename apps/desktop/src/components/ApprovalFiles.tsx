import { FileCode, TriangleAlert } from "lucide-react";
import type { ApprovalRequest } from "@/lib/api";
import { timeAgo } from "@/lib/time";
import { cn } from "@/lib/utils";

/** Changed this recently, a file may have just been written by the agent. */
const RECENT_MS = 10 * 60 * 1000;

/** The scripts an agent's command runs, and when each last changed. The
 * command line says nothing about what a script does, and the agent may
 * have written it moments before asking. */
export function ApprovalFiles({ files, cwd }: { files: ApprovalRequest["files"]; cwd: string }) {
  if (files.length === 0) return null;
  const recent = files.some((f) => f.modifiedMs !== null && Date.now() - f.modifiedMs < RECENT_MS);
  const shown = (path: string) => (path.startsWith(`${cwd}/`) ? path.slice(cwd.length + 1) : path);
  return (
    <div className="flex flex-col gap-1.5">
      <p className="text-xs font-medium text-muted-foreground">It runs {files.length === 1 ? "this file" : "these files"}</p>
      <ul className="flex flex-col gap-1">
        {files.map((f) => {
          const fresh = f.modifiedMs !== null && Date.now() - f.modifiedMs < RECENT_MS;
          return (
            <li key={f.path} className="flex items-center gap-2 rounded-xl bg-muted/60 px-3 py-1.5 text-xs">
              {fresh ? (
                <TriangleAlert aria-hidden className="size-3.5 shrink-0 text-destructive" />
              ) : (
                <FileCode aria-hidden className="size-3.5 shrink-0 text-muted-foreground" />
              )}
              <span className="selectable min-w-0 flex-1 truncate font-mono" title={f.path}>
                {shown(f.path)}
              </span>
              {f.modifiedMs !== null && (
                <span className={cn("shrink-0", fresh ? "font-medium text-destructive" : "text-muted-foreground")}>
                  Changed {timeAgo(f.modifiedMs)}
                </span>
              )}
            </li>
          );
        })}
      </ul>
      <p className="text-xs text-muted-foreground">
        {recent && "Changed just now, maybe by the agent. Read it before allowing. "}
        If it changes later, dotsafe asks again.
      </p>
    </div>
  );
}
