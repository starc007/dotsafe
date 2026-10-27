import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { ChevronRight, KeyRound, Share2, X } from "lucide-react";
import { api, message, type SecretView } from "@/lib/api";
import { daysAgo } from "@/lib/time";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Kbd } from "@/components/ui/kbd";
import { Skeleton } from "@/components/ui/skeleton";
import { DragRegion } from "@/components/DragRegion";
import { SecretDialog } from "@/components/SecretDialog";
import { Surface, SurfaceBody, SurfaceDescription, SurfaceHeader, SurfaceTitle } from "@/components/Surface";

/** Every secret: age, rotation status, and where it's linked. Esc closes. */
export function SecretsPage({
  openSecretId,
  onOpenSecret,
  onClose,
  onOpenEnvironment,
}: {
  /** The secret whose dialog is open, if any. */
  openSecretId: string | null;
  onOpenSecret: (id: string | null) => void;
  onClose: () => void;
  onOpenEnvironment: (projectId: string, environmentId: string) => void;
}) {
  const [secrets, setSecrets] = useState<SecretView[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [dialogId, setDialogId] = useState<string | null>(openSecretId);

  const load = useCallback(async () => {
    try {
      setSecrets(await api.listSecrets());
      setError(null);
    } catch (e) {
      setError(message(e));
    }
  }, []);

  useEffect(() => {
    load();
    const unlisten = listen("vault-synced", load);
    return () => {
      unlisten.then((f) => f());
    };
  }, [load]);

  // Keep the dialog's contents while it animates closed.
  useEffect(() => {
    if (openSecretId) setDialogId(openSecretId);
  }, [openSecretId]);

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || document.querySelector('[role="dialog"][data-state="open"], [role="alertdialog"][data-state="open"]')) return;
      onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onClose]);

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <header data-tauri-drag-region className="flex h-12 shrink-0 items-center gap-3 border-b px-4">
        <h1 data-tauri-drag-region className="text-sm font-semibold">
          Secrets
        </h1>
        <DragRegion className="flex-1 self-stretch" />
        <Button variant="ghost" size="sm" onClick={onClose} aria-keyshortcuts="Escape" className="text-muted-foreground">
          <X aria-hidden />
          Close
          <Kbd aria-hidden>Esc</Kbd>
        </Button>
      </header>

      <div className="relative min-h-0 flex-1 overflow-y-auto p-4">
        <div className="mx-auto flex max-w-2xl flex-col gap-4 pb-4">
          <p className="px-1 text-muted-foreground">
            A secret keeps its history and can be rotated in one step. Link it into other environments and rotating updates all of them.
          </p>
          <Surface aria-labelledby="secrets-title">
            <SurfaceHeader icon={<KeyRound aria-hidden />}>
              <SurfaceTitle id="secrets-title">Secrets</SurfaceTitle>
              <SurfaceDescription>
                {secrets === null ? " " : `${secrets.length} ${secrets.length === 1 ? "secret" : "secrets"}`}
              </SurfaceDescription>
            </SurfaceHeader>
            <SurfaceBody className="overflow-hidden p-0">
              {error ? (
                <div className="flex flex-col items-center gap-3 px-6 py-10 text-center">
                  <p className="text-sm font-semibold">Couldn't load secrets</p>
                  <p className="selectable text-muted-foreground">{error}</p>
                  <Button variant="outline" onClick={load}>
                    Try again
                  </Button>
                </div>
              ) : secrets === null ? (
                <div aria-hidden className="flex flex-col gap-3 p-4">
                  <Skeleton className="h-5 w-2/3" />
                  <Skeleton className="h-5 w-1/2" />
                </div>
              ) : secrets.length === 0 ? (
                <div className="flex flex-col items-center gap-3 px-6 py-12 text-center">
                  <div className="flex size-11 items-center justify-center rounded-xl bg-muted text-muted-foreground">
                    <Share2 aria-hidden className="size-5" />
                  </div>
                  <div>
                    <p className="text-sm font-semibold">No secrets yet</p>
                    <p className="mt-1 max-w-sm text-muted-foreground">
                      Hover a variable and choose Make it a secret. Other environments with the same value can link to it too.
                    </p>
                  </div>
                </div>
              ) : (
                <ul aria-label="Secrets" className="divide-y">
                  {secrets.map((s) => (
                    <li key={s.id}>
                      <button
                        type="button"
                        onClick={() => onOpenSecret(s.id)}
                        className="flex w-full items-center gap-3 px-4 py-2.5 text-left outline-none hover:bg-muted/50 focus-visible:bg-muted/50"
                      >
                        <span className="min-w-0 flex-1">
                          <span className="block truncate font-mono text-[12.5px] font-medium">{s.name}</span>
                          <span className="block truncate text-xs text-muted-foreground">
                            Rotated {s.rotatedAt ? daysAgo(s.rotatedAt) : "never"} · used in {s.usedIn.length}{" "}
                            {s.usedIn.length === 1 ? "place" : "places"}
                          </span>
                        </span>
                        <StatusBadge secret={s} />
                        <ChevronRight aria-hidden className="size-4 shrink-0 text-muted-foreground" />
                      </button>
                    </li>
                  ))}
                </ul>
              )}
            </SurfaceBody>
          </Surface>
        </div>
      </div>

      <SecretDialog
        secretId={dialogId}
        open={openSecretId !== null}
        onOpenChange={(open) => !open && onOpenSecret(null)}
        onChanged={load}
        onOpenEnvironment={onOpenEnvironment}
      />
    </div>
  );
}

function StatusBadge({ secret }: { secret: SecretView }) {
  const status = secret.staged
    ? { label: "Staged", className: "bg-foreground/10 text-foreground" }
    : secret.due === "overdue"
      ? { label: "Overdue", className: "bg-destructive/10 text-destructive" }
      : secret.hasPrevious
        ? { label: "Old value valid", className: "bg-muted text-foreground" }
        : secret.due === "soon"
          ? { label: "Due soon", className: "bg-primary/10 text-primary" }
          : null;
  if (!status) return null;
  return (
    <span className={cn("shrink-0 rounded-full px-2 py-0.5 text-[11px] font-medium", status.className)}>
      {status.label}
    </span>
  );
}
