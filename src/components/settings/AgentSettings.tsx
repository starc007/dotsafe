import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { toast } from "sonner";
import { Bot, Check, Copy, History, ShieldCheck } from "lucide-react";
import { api, message, type AgentActivity } from "@/lib/api";
import { daysAgo } from "@/lib/time";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { IconButton } from "@/components/IconButton";
import { ActionSwapIcon } from "@/components/motion/action-swap";
import { Surface, SurfaceBody, SurfaceDescription, SurfaceHeader, SurfaceTitle } from "@/components/Surface";

const SETUP = "claude mcp add dotsafe -- dotsafe mcp";

/** Connecting agents, the commands they may run without asking, and what
 * they've asked for recently. */
export function AgentSettings() {
  const [activity, setActivity] = useState<AgentActivity | null>(null);
  const [copied, setCopied] = useState(false);

  const load = useCallback(async () => {
    try {
      setActivity(await api.agentActivity());
    } catch (e) {
      toast.error(message(e));
    }
  }, []);

  useEffect(() => {
    load();
    const unlisten = listen("agent-activity", load);
    return () => {
      unlisten.then((f) => f());
    };
  }, [load]);

  async function copySetup() {
    // The setup command is not a secret, so the web clipboard is fine here.
    await navigator.clipboard.writeText(SETUP);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1500);
  }

  async function revoke() {
    try {
      await api.revokeAgentGrants();
      toast("Agents will ask again for every command");
      load();
    } catch (e) {
      toast.error(message(e));
    }
  }

  return (
    <>
      <Surface aria-labelledby="agents-setup-title">
        <SurfaceHeader icon={<Bot aria-hidden />}>
          <SurfaceTitle id="agents-setup-title">Connect an agent</SurfaceTitle>
          <SurfaceDescription>Agents see variable names and ask before running anything.</SurfaceDescription>
        </SurfaceHeader>
        <SurfaceBody className="flex flex-col gap-3">
          <p className="text-muted-foreground">
            Add dotsafe as an MCP server. In Claude Code, run this in a terminal (with the dotsafe CLI installed):
          </p>
          <div className="flex items-center gap-1 rounded-full bg-muted/60 py-1 pr-1 pl-3.5">
            <code className="selectable min-w-0 flex-1 truncate font-mono text-[12.5px]">{SETUP}</code>
            <IconButton label="Copy setup command" onClick={copySetup}>
              <ActionSwapIcon value={copied ? "copied" : "copy"} animation="blur" className="size-4">
                {copied ? <Check aria-hidden /> : <Copy aria-hidden />}
              </ActionSwapIcon>
            </IconButton>
          </div>
          <p className="text-xs text-muted-foreground">
            For Cursor and other agents, <code className="font-mono">dotsafe mcp setup</code> prints the config to add.
            Agents can only reach dotsafe while this app is open and unlocked.
          </p>
        </SurfaceBody>
      </Surface>

      <Surface aria-labelledby="agents-grants-title">
        <SurfaceHeader
          icon={<ShieldCheck aria-hidden />}
          action={
            activity && activity.grants.length > 0 ? (
              <Button variant="outline" className="bg-card" onClick={revoke}>
                Revoke all
              </Button>
            ) : undefined
          }
        >
          <SurfaceTitle id="agents-grants-title">Allowed without asking</SurfaceTitle>
          <SurfaceDescription>Commands you allowed for a while. They all end when dotsafe locks.</SurfaceDescription>
        </SurfaceHeader>
        <SurfaceBody className={cn(activity?.grants.length ? "p-0" : undefined)}>
          {activity === null ? (
            <Skeleton aria-hidden className="h-5 w-1/2" />
          ) : activity.grants.length === 0 ? (
            <p className="text-muted-foreground">Nothing. Every command asks first.</p>
          ) : (
            <ul className="divide-y">
              {activity.grants.map((g, i) => (
                <li key={i} className="flex items-center gap-3 px-4 py-2.5">
                  <span className="min-w-0 flex-1">
                    <span className="block truncate font-mono text-[12.5px]">{g.command}</span>
                    <span className="block truncate text-xs text-muted-foreground">{g.cwd}</span>
                  </span>
                  <span className="shrink-0 text-xs text-muted-foreground">
                    {g.expiresIn === null ? "Until lock" : `${Math.max(1, Math.round(g.expiresIn / 60))} min left`}
                  </span>
                </li>
              ))}
            </ul>
          )}
        </SurfaceBody>
      </Surface>

      <Surface aria-labelledby="agents-recent-title">
        <SurfaceHeader icon={<History aria-hidden />}>
          <SurfaceTitle id="agents-recent-title">Activity</SurfaceTitle>
          <SurfaceDescription>From every device, kept encrypted in your vault for 90 days.</SurfaceDescription>
        </SurfaceHeader>
        <SurfaceBody className={cn(activity?.recent.length ? "p-0" : undefined)}>
          {activity === null ? (
            <Skeleton aria-hidden className="h-5 w-2/3" />
          ) : activity.recent.length === 0 ? (
            <p className="text-muted-foreground">No requests yet.</p>
          ) : (
            <ul className="divide-y">
              {activity.recent.map((e, i) => (
                <li key={i} className="flex items-center gap-3 px-4 py-2.5">
                  <span className="min-w-0 flex-1">
                    <span className="block truncate font-mono text-[12.5px]">
                      {e.action === "generate" ? `Added ${e.summary}` : e.summary}
                    </span>
                    <span className="block truncate text-xs text-muted-foreground">
                      {e.actor}
                      {e.environment && ` · ${e.environment}`} · {daysAgo(e.at)}
                      {e.detail && ` · ${e.detail}`}
                    </span>
                  </span>
                  <span
                    className={cn(
                      "shrink-0 rounded-full px-2 py-0.5 text-[11px] font-medium capitalize",
                      e.outcome === "allowed" && "bg-primary/10 text-primary",
                      e.outcome === "refused" && "bg-destructive/10 text-destructive",
                      (e.outcome === "denied" || e.outcome === "failed") && "bg-muted text-muted-foreground",
                    )}
                  >
                    {e.outcome}
                  </span>
                </li>
              ))}
            </ul>
          )}
        </SurfaceBody>
      </Surface>
    </>
  );
}
