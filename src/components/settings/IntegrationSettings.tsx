import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";
import { Plug, Plus } from "lucide-react";
import { api, message, type ConnectionView, type ServiceView } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { AddConnectionDialog } from "@/components/AddConnectionDialog";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { Surface, SurfaceBody, SurfaceDescription, SurfaceHeader, SurfaceTitle } from "@/components/Surface";

/** Connected accounts at providers and deploy targets. */
export function IntegrationSettings() {
  const [connections, setConnections] = useState<ConnectionView[] | null>(null);
  const [services, setServices] = useState<ServiceView[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [adding, setAdding] = useState(false);
  const [removing, setRemoving] = useState<ConnectionView | null>(null);
  const [busy, setBusy] = useState(false);

  const load = useCallback(async () => {
    try {
      const [list, catalogue] = await Promise.all([api.listConnections(), api.listServices()]);
      setConnections(list);
      setServices(catalogue);
      setError(null);
    } catch (e) {
      setError(message(e));
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  async function remove() {
    if (!removing) return;
    setBusy(true);
    try {
      await api.removeConnection(removing.id);
      toast(`Removed ${removing.name}`, { description: "Revoke its token at the service too." });
      setRemoving(null);
      load();
    } catch (e) {
      toast.error(message(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <>
      <Surface aria-labelledby="connections-title">
        <SurfaceHeader
          icon={<Plug aria-hidden />}
          action={
            <Button variant="outline" className="bg-card" onClick={() => setAdding(true)} disabled={!services.length}>
              <Plus aria-hidden />
              Add a connection
            </Button>
          }
        >
          <SurfaceTitle id="connections-title">Connections</SurfaceTitle>
          <SurfaceDescription>
            Providers issue a new key when a secret rotates. Deploy targets get the new value pushed.
          </SurfaceDescription>
        </SurfaceHeader>
        <SurfaceBody className={connections?.length ? "p-0" : undefined}>
          {error ? (
            <div className="flex items-center justify-between gap-3">
              <p className="text-destructive">{error}</p>
              <Button variant="outline" onClick={load}>
                Try again
              </Button>
            </div>
          ) : connections === null ? (
            <Skeleton aria-hidden className="h-5 w-2/3" />
          ) : connections.length === 0 ? (
            <p className="text-muted-foreground">
              No connections yet. Connect OpenAI or a Cloudflare token to rotate keys in one click, and Vercel or
              Cloudflare Workers to push every rotation.
            </p>
          ) : (
            <ul className="divide-y">
              {connections.map((c) => (
                <li key={c.id} className="flex items-center gap-3 px-4 py-2.5">
                  <span className="min-w-0 flex-1">
                    <span className="block truncate font-medium">{c.name}</span>
                    <span className="block truncate text-xs text-muted-foreground">
                      {c.serviceLabel} · {c.kind === "provider" ? "issues keys" : "deploy target"}
                      {c.usedBy > 0 && ` · used by ${c.usedBy} ${c.usedBy === 1 ? "secret" : "secrets"}`}
                    </span>
                  </span>
                  <Button
                    variant="ghost"
                    className="text-destructive hover:text-destructive"
                    onClick={() => setRemoving(c)}
                  >
                    Remove…
                  </Button>
                </li>
              ))}
            </ul>
          )}
        </SurfaceBody>
      </Surface>
      <AddConnectionDialog open={adding} services={services} onOpenChange={setAdding} onAdded={load} />
      <ConfirmDialog
        open={removing !== null}
        busy={busy}
        title={`Remove ${removing?.name}?`}
        description={
          removing && removing.usedBy > 0
            ? `${removing.usedBy === 1 ? "A secret uses" : `${removing.usedBy} secrets use`} it. Remove it from them first.`
            : "Its stored token is deleted from the vault. Revoke the token at the service too."
        }
        confirmLabel="Remove"
        onConfirm={remove}
        onCancel={() => setRemoving(null)}
      />
    </>
  );
}
