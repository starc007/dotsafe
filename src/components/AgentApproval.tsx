import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { toast } from "sonner";
import { ArrowLeftRight, Bot, Sparkles } from "lucide-react";
import { api, message, type ApprovalRequest, type GrantLifetime } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Tabs, TabsList, TabsTrigger } from "@/components/motion/tabs";
import { SurfaceDialog } from "@/components/SurfaceDialog";
import { SurfaceFooter } from "@/components/Surface";

const LIFETIMES: { value: GrantLifetime; label: string }[] = [
  { value: "once", label: "Once" },
  { value: "15m", label: "15 min" },
  { value: "1h", label: "1 hour" },
  { value: "lock", label: "Until lock" },
];

/** Asks before an agent's command gets secrets. Requests queue up and are
 * shown one at a time; closing the dialog denies. */
export function AgentApproval() {
  const [queue, setQueue] = useState<ApprovalRequest[]>([]);
  const [lifetime, setLifetime] = useState<GrantLifetime>("once");
  const [busy, setBusy] = useState(false);
  const current = queue[0] ?? null;
  // Keep showing the last request while the dialog animates closed.
  const [shown, setShown] = useState<ApprovalRequest | null>(null);
  useEffect(() => {
    if (current) setShown(current);
  }, [current]);

  useEffect(() => {
    const unlisteners = [
      listen<ApprovalRequest>("agent-request", (e) => setQueue((q) => [...q, e.payload])),
      // Timed out on the broker's side: drop it.
      listen<string>("agent-request-closed", (e) => setQueue((q) => q.filter((r) => r.id !== e.payload))),
    ];
    return () => unlisteners.forEach((u) => u.then((f) => f()));
  }, []);

  async function answer(allow: boolean) {
    if (!current) return;
    setBusy(true);
    try {
      await api.answerAgentRequest(current.id, allow ? lifetime : null);
    } catch (e) {
      toast.error(message(e));
    } finally {
      setQueue((q) => q.slice(1));
      setLifetime("once");
      setBusy(false);
    }
  }

  const request = current ?? shown;
  return (
    <SurfaceDialog
      open={current !== null}
      onOpenChange={(open) => !open && answer(false)}
      icon={request?.kind === "generate" ? <Sparkles aria-hidden /> : <Bot aria-hidden />}
      title={
        !request
          ? "Allow this?"
          : request.kind === "generate"
            ? `Let ${request.client} add ${request.variables[0]}?`
            : `Allow ${request.client} to run this?`
      }
      description={
        !request
          ? undefined
          : request.kind === "generate"
            ? `To ${request.project}/${request.environment}`
            : `With ${request.variables.length} variables from ${request.project}/${request.environment}`
      }
      className="sm:max-w-lg"
    >
      {request?.kind === "generate" && (
        <div className="flex flex-col gap-4">
          <p className="text-muted-foreground">
            dotsafe will create <span className="font-mono text-foreground">{request.variables[0]}</span> with{" "}
            {request.generated}. The value is made here and never shown to {request.client}; commands you allow later
            can use it.
          </p>
          <SurfaceFooter className="pt-1">
            <Button variant="ghost" onClick={() => answer(false)} disabled={busy}>
              Deny
            </Button>
            <Button onClick={() => answer(true)} disabled={busy}>
              Add it
            </Button>
          </SurfaceFooter>
        </div>
      )}
      {request?.kind === "run" && (
        <div className="flex flex-col gap-4">
          <div className="flex flex-col gap-1.5">
            <pre className="selectable overflow-x-auto rounded-2xl bg-muted/60 px-3.5 py-2.5 font-mono text-[12.5px] whitespace-pre-wrap">
              {request.command.join(" ")}
            </pre>
            <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-0.5 px-1 text-xs text-muted-foreground">
              <dt>Program</dt>
              <dd className="selectable truncate font-mono" title={request.programPath}>
                {request.programPath}
              </dd>
              <dt>In</dt>
              <dd className="selectable truncate font-mono" title={request.cwd}>
                {request.cwd}
              </dd>
            </dl>
          </div>

          <div className="flex flex-col gap-1.5">
            <p className="text-xs font-medium text-muted-foreground">It will get these variables</p>
            <ul className="flex max-h-28 flex-wrap gap-1 overflow-y-auto">
              {request.variables.map((v) =>
                request.proxied.includes(v) ? (
                  <li
                    key={v}
                    title="Goes through dotsafe's proxy: the process gets a placeholder, never the key."
                    className="flex items-center gap-1 rounded-full bg-primary/10 px-2 py-0.5 font-mono text-[11.5px] text-primary"
                  >
                    <ArrowLeftRight aria-hidden className="size-3" />
                    {v}
                  </li>
                ) : (
                  <li key={v} className="rounded-full bg-muted px-2 py-0.5 font-mono text-[11.5px]">
                    {v}
                  </li>
                ),
              )}
              {request.variables.length === 0 && <li className="text-muted-foreground">None</li>}
            </ul>
            <p className="text-xs text-muted-foreground">
              The agent never sees the values, and they're scrubbed from what it reads back.
              {request.proxied.length > 0 &&
                ` ${request.proxied.join(" and ")} ${request.proxied.length === 1 ? "goes" : "go"} through dotsafe's proxy, so the command never holds ${request.proxied.length === 1 ? "that key" : "those keys"}.`}{" "}
              The command can use the rest however it's written, so only allow commands you'd run yourself.
            </p>
          </div>

          <div className="flex flex-col gap-1.5">
            <p className="text-xs font-medium text-muted-foreground">Allow this exact command</p>
            <Tabs value={lifetime} onValueChange={(v) => setLifetime(v as GrantLifetime)}>
              <TabsList aria-label="How long to allow it" className="w-full bg-muted p-0.5">
                {LIFETIMES.map((l) => (
                  <TabsTrigger key={l.value} value={l.value} className="flex-1 py-1 text-[13px]">
                    {l.label}
                  </TabsTrigger>
                ))}
              </TabsList>
            </Tabs>
          </div>

          <SurfaceFooter className="pt-1">
            <Button variant="ghost" onClick={() => answer(false)} disabled={busy}>
              Deny
            </Button>
            <Button onClick={() => answer(true)} disabled={busy}>
              Allow
            </Button>
          </SurfaceFooter>
        </div>
      )}
    </SurfaceDialog>
  );
}
