import { useState } from "react";
import { toast } from "sonner";
import { KeyRound, Lock, RefreshCw, ShieldCheck } from "lucide-react";
import { api, message, type Settings } from "@/lib/api";
import { displayPath } from "@/lib/paths";
import { SHORTCUTS, ariaShortcut } from "@/lib/shortcuts";
import { Button } from "@/components/ui/button";
import { Spinner } from "@/components/ui/spinner";
import { ChangePasswordDialog } from "@/components/ChangePasswordDialog";
import { NewRecoveryKeyDialog } from "@/components/NewRecoveryKeyDialog";
import { SettingsRow } from "@/components/SettingsRow";
import { ShortcutKeys } from "@/components/ShortcutKeys";
import { Surface, SurfaceBody, SurfaceDescription, SurfaceHeader, SurfaceTitle } from "@/components/Surface";

export function SecuritySettings({
  settings,
  onLock,
  onChanged,
}: {
  settings: Settings;
  onLock: () => void;
  onChanged: () => void;
}) {
  const [dialog, setDialog] = useState<"password" | "recovery" | null>(null);
  const [syncing, setSyncing] = useState(false);
  const inBucket = settings.storage === "s3";

  async function syncNow() {
    setSyncing(true);
    try {
      await api.syncNow();
      toast.success("Synced");
    } catch (e) {
      toast.error(message(e));
    } finally {
      setSyncing(false);
      onChanged();
    }
  }
  return (
    <>
      <Surface aria-labelledby="access-title">
        <SurfaceHeader icon={<KeyRound aria-hidden />}>
          <SurfaceTitle id="access-title">Access</SurfaceTitle>
          <SurfaceDescription>How you get into this vault.</SurfaceDescription>
        </SurfaceHeader>
        <SurfaceBody className="divide-y">
          <SettingsRow label="Master password" detail="Unlocks this vault on every device.">
            <Button variant="outline" onClick={() => setDialog("password")}>
              Change…
            </Button>
          </SettingsRow>
          <SettingsRow label="Recovery key" detail="Gets you back in if you forget your password.">
            <Button variant="outline" onClick={() => setDialog("recovery")}>
              Create new key…
            </Button>
          </SettingsRow>
          <SettingsRow label="Lock now" detail="You'll need your master password to get back in.">
            <Button variant="outline" onClick={onLock} aria-keyshortcuts={ariaShortcut(SHORTCUTS.lock)}>
              <Lock aria-hidden />
              Lock
              <ShortcutKeys shortcut={SHORTCUTS.lock} />
            </Button>
          </SettingsRow>
        </SurfaceBody>
      </Surface>

      <Surface aria-labelledby="protection-title">
        <SurfaceHeader icon={<ShieldCheck aria-hidden />}>
          <SurfaceTitle id="protection-title">Protection</SurfaceTitle>
          <SurfaceDescription>Where your vault lives and how dotsafe keeps it safe.</SurfaceDescription>
        </SurfaceHeader>
        <SurfaceBody className="divide-y">
          <SettingsRow
            label="Vault location"
            detail={
              inBucket
                ? "Your bucket only ever holds encrypted files. A copy is cached on this Mac for offline use."
                : "Synced by the service that owns this folder."
            }
          >
            <span title={settings.vaultPath ?? undefined} className="selectable block max-w-56 truncate">
              {settings.vaultPath ? displayPath(settings.vaultPath) : "Not set up"}
            </span>
          </SettingsRow>
          {inBucket && (
            <SettingsRow label="Sync" detail={settings.syncError ?? syncedLabel(settings.lastSyncedAt)}>
              <Button variant="outline" onClick={syncNow} disabled={syncing}>
                {syncing ? <Spinner /> : <RefreshCw aria-hidden />}
                Sync now
              </Button>
            </SettingsRow>
          )}
          <SettingsRow label="Auto-lock" detail="Locks when you haven't used dotsafe for a while.">
            After {settings.autoLockMinutes} minutes
          </SettingsRow>
          <SettingsRow label="Clipboard" detail="Copied values are cleared unless you've copied something else since.">
            After {settings.clipboardClearSeconds} seconds
          </SettingsRow>
        </SurfaceBody>
      </Surface>

      <ChangePasswordDialog open={dialog === "password"} onOpenChange={(open) => !open && setDialog(null)} />
      <NewRecoveryKeyDialog open={dialog === "recovery"} onOpenChange={(open) => !open && setDialog(null)} />
    </>
  );
}

function syncedLabel(at: number | null): string {
  if (!at) return "Syncs every minute while dotsafe is unlocked.";
  const minutes = Math.round((Date.now() - at) / 60_000);
  if (minutes < 1) return "Synced just now.";
  return `Synced ${new Intl.RelativeTimeFormat(undefined, { numeric: "auto" }).format(-minutes, "minute")}.`;
}
