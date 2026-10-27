import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";
import { ListChecks, RotateCcw } from "lucide-react";
import { api, message, type QueueView } from "@/lib/api";
import { formatDate } from "@/lib/time";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { Spinner } from "@/components/ui/spinner";
import { reportPushes } from "@/components/SecretIntegrationsSection";
import { SurfaceDialog } from "@/components/SurfaceDialog";
import { SurfaceFooter } from "@/components/Surface";

/** Everything a removed device could still know, until it's rotated:
 * provider-backed secrets in one click, the rest one by one. */
export function RotationQueueDialog({
  open,
  onOpenChange,
  onChanged,
  onOpenSecret,
  onOpenEnvironment,
  onOpenIntegrations,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onChanged: () => void;
  onOpenSecret: (id: string) => void;
  onOpenEnvironment: (projectId: string, environmentId: string) => void;
  onOpenIntegrations: () => void;
}) {
  const [queue, setQueue] = useState<QueueView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<"rotate" | "dismiss" | null>(null);

  const load = useCallback(async () => {
    try {
      setQueue(await api.rotationQueue());
      setError(null);
    } catch (e) {
      setError(message(e));
    }
  }, []);

  useEffect(() => {
    if (!open) return;
    setQueue(null);
    load();
  }, [open, load]);

  async function rotateWithProviders() {
    setBusy("rotate");
    try {
      for (const r of await api.rotateQueuedWithProviders()) {
        if (r.error) toast.error(`Couldn't rotate ${r.name}`, { description: r.error });
        else toast.success(`Rotated ${r.name}`);
        reportPushes(r.pushes);
      }
      load();
      onChanged();
    } catch (e) {
      toast.error(message(e));
    } finally {
      setBusy(null);
    }
  }

  async function dismiss() {
    setBusy("dismiss");
    try {
      await api.dismissRotationQueue();
      onChanged();
      onOpenChange(false);
    } catch (e) {
      toast.error(message(e));
    } finally {
      setBusy(null);
    }
  }

  const withProvider = queue?.secrets.filter((s) => s.hasProvider).length ?? 0;
  const left = (queue?.secrets.length ?? 0) + (queue?.variables.length ?? 0);
  const go = (fn: () => void) => () => {
    onOpenChange(false);
    fn();
  };

  return (
    <SurfaceDialog
      open={open}
      onOpenChange={onOpenChange}
      icon={<ListChecks aria-hidden />}
      title={queue ? `Rotate what ${queue.device} could read` : "Rotation queue"}
      description={
        queue
          ? `Removed ${formatDate(queue.since)}. It still knows every value it had then. Each one leaves this list once it's rotated.`
          : " "
      }
      className="sm:max-w-lg"
    >
      {error ? (
        <p className="text-destructive">{error}</p>
      ) : !queue ? (
        <div className="flex flex-col gap-2">
          <Skeleton className="h-16 w-full rounded-2xl" />
          <Skeleton className="h-24 w-full rounded-2xl" />
        </div>
      ) : (
        <div className="-mx-5 -my-5 flex max-h-[min(70vh,600px)] flex-col gap-4 overflow-y-auto p-5">
          {left === 0 ? (
            <p className="text-muted-foreground">Everything it could read has been rotated.</p>
          ) : (
            <>
              {withProvider > 0 && (
                <div role="status" className="flex items-center gap-3 rounded-2xl bg-muted/60 p-3.5">
                  <p className="min-w-0 flex-1 text-[13px]">
                    <span className="font-medium">
                      {withProvider} {withProvider === 1 ? "secret has" : "secrets have"} a provider.
                    </span>{" "}
                    <span className="text-muted-foreground">dotsafe can get new keys and push them for you.</span>
                  </p>
                  <Button onClick={rotateWithProviders} disabled={busy !== null}>
                    {busy === "rotate" ? <Spinner /> : <RotateCcw aria-hidden />}
                    Rotate {withProvider}
                  </Button>
                </div>
              )}
              {queue.secrets.length > 0 && (
                <QueueList title="Shared secrets">
                  {queue.secrets.map((s) => (
                    <QueueRow
                      key={s.id}
                      label={s.name}
                      detail={s.hasProvider ? "has a provider" : s.connectionToken ? "connection token" : "rotate by hand"}
                      onClick={go(() => (s.connectionToken ? onOpenIntegrations() : onOpenSecret(s.id)))}
                    />
                  ))}
                </QueueList>
              )}
              {queue.variables.length > 0 && (
                <QueueList title="Variables">
                  {queue.variables.map((v) => (
                    <QueueRow
                      key={`${v.environmentId}/${v.key}`}
                      label={v.key}
                      detail={`${v.project}/${v.environment}`}
                      mono
                      onClick={go(() => onOpenEnvironment(v.projectId, v.environmentId))}
                    />
                  ))}
                </QueueList>
              )}
            </>
          )}
          <SurfaceFooter className="pt-0">
            <Button variant="ghost" onClick={dismiss} disabled={busy !== null}>
              {busy === "dismiss" && <Spinner />}
              {left === 0 ? "Clear list" : "Stop tracking"}
            </Button>
            <Button onClick={() => onOpenChange(false)}>Done</Button>
          </SurfaceFooter>
        </div>
      )}
    </SurfaceDialog>
  );
}

function QueueList({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="flex flex-col gap-2">
      <h3 className="text-xs font-medium text-muted-foreground">{title}</h3>
      <ul className="flex flex-col divide-y rounded-2xl bg-muted/40">{children}</ul>
    </section>
  );
}

function QueueRow({ label, detail, mono, onClick }: { label: string; detail: string; mono?: boolean; onClick: () => void }) {
  return (
    <li>
      <button
        type="button"
        onClick={onClick}
        className="flex w-full items-center gap-3 rounded-2xl px-3 py-2 text-left outline-none hover:bg-muted focus-visible:ring-2 focus-visible:ring-ring"
      >
        <span className={mono ? "min-w-0 flex-1 truncate font-mono text-[12px]" : "min-w-0 flex-1 truncate"}>{label}</span>
        <span className="truncate text-xs text-muted-foreground">{detail}</span>
      </button>
    </li>
  );
}
