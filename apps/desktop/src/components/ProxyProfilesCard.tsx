import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";
import { Plus, Route, X } from "lucide-react";
import { api, message, type ProxyProfileView } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { IconButton } from "@/components/IconButton";
import { ProxyProfileDialog } from "@/components/ProxyProfileDialog";
import { Surface, SurfaceBody, SurfaceDescription, SurfaceHeader, SurfaceTitle } from "@/components/Surface";

/** The API keys the secret proxy keeps out of processes: built in, and
 * custom profiles from the vault. */
export function ProxyProfilesCard() {
  const [profiles, setProfiles] = useState<ProxyProfileView[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [adding, setAdding] = useState(false);

  const load = useCallback(async () => {
    try {
      setProfiles(await api.listProxyProfiles());
      setError(null);
    } catch (e) {
      setError(message(e));
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  async function remove(p: ProxyProfileView) {
    if (!p.id) return;
    try {
      await api.removeProxyProfile(p.id);
      toast(`Removed ${p.name}`);
      load();
    } catch (e) {
      toast.error(message(e));
    }
  }

  return (
    <>
      <Surface aria-labelledby="proxy-profiles-title">
        <SurfaceHeader
          icon={<Route aria-hidden />}
          action={
            <Button variant="outline" className="bg-card" onClick={() => setAdding(true)}>
              <Plus aria-hidden />
              Add a profile
            </Button>
          }
        >
          <SurfaceTitle id="proxy-profiles-title">Proxy profiles</SurfaceTitle>
          <SurfaceDescription>
            Keys that go through the secret proxy, so commands and agents use them without holding them.
          </SurfaceDescription>
        </SurfaceHeader>
        <SurfaceBody className={profiles?.length ? "p-0" : undefined}>
          {error ? (
            <p className="text-destructive">{error}</p>
          ) : profiles === null ? (
            <Skeleton aria-hidden className="h-5 w-2/3" />
          ) : (
            <ul className="divide-y">
              {profiles.map((p) => (
                <li key={p.id ?? p.keyVar} className="flex items-center gap-3 px-4 py-2.5">
                  <span className="min-w-0 flex-1">
                    <span className="block truncate font-medium">
                      {p.name} <span className="font-mono text-[12px] font-normal text-muted-foreground">{p.keyVar}</span>
                    </span>
                    <span className="block truncate text-xs text-muted-foreground">
                      {p.problem ? `Not used: ${p.problem}` : `${p.upstream}${p.id ? "" : " · built in"}`}
                    </span>
                  </span>
                  {p.id && (
                    <IconButton label={`Remove ${p.name}`} onClick={() => remove(p)}>
                      <X aria-hidden />
                    </IconButton>
                  )}
                </li>
              ))}
            </ul>
          )}
        </SurfaceBody>
      </Surface>
      <ProxyProfileDialog open={adding} onOpenChange={setAdding} onAdded={load} />
    </>
  );
}
