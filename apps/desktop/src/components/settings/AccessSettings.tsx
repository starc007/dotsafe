import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { toast } from "sonner";
import { Database, Plus, RefreshCw, Server } from "lucide-react";
import { api, message, type SharesView, type ShareView } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { Spinner } from "@/components/ui/spinner";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { NewShareDialog } from "@/components/NewShareDialog";
import { ShareBucketDialog } from "@/components/ShareBucketDialog";
import { LEASE_WARNING_DAYS, ShareRow } from "@/components/ShareRow";
import { Surface, SurfaceBody, SurfaceDescription, SurfaceHeader, SurfaceTitle } from "@/components/Surface";

/** Shares: machines (CI, servers, cloud agents) that read some environments
 * from a sealed snapshot in the share bucket, with one token. */
export function AccessSettings() {
  const [view, setView] = useState<SharesView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [settingUp, setSettingUp] = useState(false);
  const [creating, setCreating] = useState(false);
  const [revoking, setRevoking] = useState<ShareView | null>(null);
  const [busy, setBusy] = useState<"revoke" | "push" | null>(null);

  const load = useCallback(async () => {
    try {
      setView(await api.listShares());
      setError(null);
    } catch (e) {
      setError(message(e));
    }
  }, []);

  useEffect(() => {
    load();
    // The background pusher updated a snapshot, or another device's changes arrived.
    const unlisteners = [listen("shares-changed", load), listen("vault-synced", load)];
    return () => {
      unlisteners.forEach((u) => u.then((f) => f()));
    };
  }, [load]);

  async function revoke() {
    if (!revoking) return;
    setBusy("revoke");
    try {
      const failed = await api.revokeShare(revoking.id);
      if (failed) toast(`Revoked ${revoking.name}`, { description: `Its snapshot is deleted once this Mac can reach the share bucket. ${failed}` });
      else toast(`Revoked ${revoking.name}`, { description: "Its token no longer works." });
      setRevoking(null);
      load();
      // The sidebar picks up what to rotate.
      window.dispatchEvent(new Event("dotsafe:vault-changed"));
    } catch (e) {
      toast.error(message(e));
    } finally {
      setBusy(null);
    }
  }

  async function pushNow() {
    setBusy("push");
    try {
      const results = await api.pushShares();
      const failed = results.filter((r) => r.error);
      for (const r of failed) toast.error(`Couldn't push ${r.name}`, { description: r.error });
      if (!failed.length) toast.success(results.length ? "Shares are up to date" : "Every share was already up to date");
      load();
    } catch (e) {
      toast.error(message(e));
    } finally {
      setBusy(null);
    }
  }

  // Out of date, or with a lease about to run out.
  const behind =
    view?.shares.filter(
      (s) =>
        s.status === "outOfDate" ||
        s.status === "neverPushed" ||
        (s.leaseDaysLeft !== null && s.leaseDaysLeft < LEASE_WARNING_DAYS),
    ).length ?? 0;

  return (
    <>
      <Surface aria-labelledby="share-bucket-title">
        <SurfaceHeader
          icon={<Database aria-hidden />}
          action={
            view && (
              <Button variant="outline" className="bg-card" onClick={() => setSettingUp(true)}>
                {view.bucket ? "Change" : "Set up"}
              </Button>
            )
          }
        >
          <SurfaceTitle id="share-bucket-title">Share bucket</SurfaceTitle>
          <SurfaceDescription>Where snapshots go. Machines read them over HTTPS.</SurfaceDescription>
        </SurfaceHeader>
        <SurfaceBody>
          {error ? (
            <div className="flex items-center justify-between gap-3">
              <p className="text-destructive">{error}</p>
              <Button variant="outline" onClick={load}>
                Try again
              </Button>
            </div>
          ) : view === null ? (
            <Skeleton aria-hidden className="h-5 w-2/3" />
          ) : view.bucket ? (
            <dl className="flex flex-col gap-1 text-[13px]">
              <div className="flex gap-3">
                <dt className="w-20 shrink-0 text-muted-foreground">Bucket</dt>
                <dd className="selectable min-w-0 truncate font-mono text-[12px]">{view.bucket.bucket}</dd>
              </div>
              <div className="flex gap-3">
                <dt className="w-20 shrink-0 text-muted-foreground">Public URL</dt>
                <dd className="selectable min-w-0 truncate font-mono text-[12px]">{view.bucket.publicUrl}</dd>
              </div>
            </dl>
          ) : (
            <p className="text-muted-foreground">
              None on this Mac yet. Use an S3-compatible bucket (R2 works) only for shares: public-read, with listing off.
            </p>
          )}
        </SurfaceBody>
      </Surface>

      <Surface aria-labelledby="shares-title">
        <SurfaceHeader
          icon={<Server aria-hidden />}
          action={
            <Button variant="outline" className="bg-card" onClick={() => setCreating(true)} disabled={!view?.bucket}>
              <Plus aria-hidden />
              New share
            </Button>
          }
        >
          <SurfaceTitle id="shares-title">Shares</SurfaceTitle>
          <SurfaceDescription>Each machine gets final values only, and only for what you pick.</SurfaceDescription>
        </SurfaceHeader>
        <SurfaceBody className={view?.shares.length ? "flex flex-col gap-0 p-0" : undefined}>
          {error ? (
            <p className="text-muted-foreground">Shares show up here once they load.</p>
          ) : view === null ? (
            <Skeleton aria-hidden className="h-5 w-2/3" />
          ) : view.shares.length === 0 ? (
            <p className="text-muted-foreground">
              {view.bucket
                ? "No shares yet. Make one for CI or a server, and give it the token."
                : "No shares yet. Set up the share bucket first."}
            </p>
          ) : (
            <>
              {behind > 0 && view.bucket && (
                <div role="status" className="m-1.5 mb-0 flex items-center gap-3 rounded-2xl bg-muted/60 p-3">
                  <p className="min-w-0 flex-1 text-[13px]">
                    <span className="font-medium">
                      {behind} {behind === 1 ? "share needs" : "shares need"} a push.
                    </span>{" "}
                    <span className="text-muted-foreground">
                      dotsafe does it on its own while it can reach the bucket.
                    </span>
                  </p>
                  <Button variant="outline" className="bg-card" onClick={pushNow} disabled={busy !== null}>
                    {busy === "push" ? <Spinner /> : <RefreshCw aria-hidden />}
                    Push now
                  </Button>
                </div>
              )}
              <ul className="divide-y">
                {view.shares.map((s) => (
                  <ShareRow key={s.id} share={s} onRevoke={() => setRevoking(s)} />
                ))}
              </ul>
            </>
          )}
        </SurfaceBody>
      </Surface>

      <ShareBucketDialog open={settingUp} current={view?.bucket ?? null} onOpenChange={setSettingUp} onSaved={load} />
      <NewShareDialog open={creating} onOpenChange={setCreating} onCreated={load} />
      <ConfirmDialog
        open={revoking !== null}
        busy={busy === "revoke"}
        title={`Revoke ${revoking?.name}?`}
        description="Its snapshot is deleted, so the token stops working. The machine keeps what it already read, so dotsafe lists those values to rotate."
        confirmLabel="Revoke"
        onConfirm={revoke}
        onCancel={() => setRevoking(null)}
      />
    </>
  );
}
