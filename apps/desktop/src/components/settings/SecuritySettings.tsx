import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { toast } from "sonner";
import { Archive, FolderInput, KeyRound, Lock, RefreshCw, ShieldCheck } from "lucide-react";
import { api, message, type Settings, type VaultHealth } from "@/lib/api";
import { displayPath } from "@/lib/paths";
import { SHORTCUTS, ariaShortcut } from "@/lib/shortcuts";
import { Button } from "@/components/ui/button";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Spinner } from "@/components/ui/spinner";
import { ChangePasswordDialog } from "@/components/ChangePasswordDialog";
import { IntegrityNotice } from "@/components/IntegrityNotice";
import { NewRecoveryKeyDialog } from "@/components/NewRecoveryKeyDialog";
import { SettingsRow } from "@/components/SettingsRow";
import { ShortcutKeys } from "@/components/ShortcutKeys";
import { thisDevice } from "@/lib/platform";
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
  const [health, setHealth] = useState<VaultHealth | null>(null);
  const inBucket = settings.storage !== "folder";
  const [backingUp, setBackingUp] = useState(false);
  const [moving, setMoving] = useState(false);

  async function chooseBackupFolder() {
    const picked = await open({ directory: true, title: "Choose where backups go" });
    if (typeof picked !== "string") return;
    try {
      await api.setBackupDir(picked);
      await api.backUpNow();
      toast.success("Backed up", { description: "dotsafe backs up to this folder once a day while unlocked." });
    } catch (e) {
      toast.error(message(e));
    } finally {
      onChanged();
    }
  }

  async function setAutoLock(minutes: number) {
    try {
      await api.setAutoLock(minutes);
      toast.success(`Auto-lock: ${autoLockLabel(minutes).toLowerCase()}`);
    } catch (e) {
      toast.error(message(e));
    } finally {
      onChanged();
    }
  }

  async function backUpNow() {
    setBackingUp(true);
    try {
      await api.backUpNow();
      toast.success("Backed up");
    } catch (e) {
      toast.error(message(e));
    } finally {
      setBackingUp(false);
      onChanged();
    }
  }

  async function moveVault() {
    const picked = await open({ directory: true, title: "Choose an empty folder for the vault" });
    if (typeof picked !== "string") return;
    setMoving(true);
    try {
      const old = await api.moveVaultToFolder(picked);
      toast.success("Vault moved", {
        description: `The old copy at ${displayPath(old)} is untouched. Connect your other devices to the new folder, then delete it.`,
      });
    } catch (e) {
      toast.error(message(e));
    } finally {
      setMoving(false);
      onChanged();
    }
  }

  useEffect(() => {
    // Not being able to check isn't worth an error of its own.
    api.vaultHealth().then(setHealth, () => {});
  }, [settings]);

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
      {health && <IntegrityNotice health={health} />}
      <Surface aria-labelledby="access-title">
        <SurfaceHeader icon={<KeyRound aria-hidden />}>
          <SurfaceTitle id="access-title">Access</SurfaceTitle>
          <SurfaceDescription>How you get into this vault.</SurfaceDescription>
        </SurfaceHeader>
        <SurfaceBody className="divide-y">
          <SettingsRow label="Master password" detail="Unlocks this vault on every device.">
            <Button variant="outline" onClick={() => setDialog("password")}>
              Change
            </Button>
          </SettingsRow>
          <SettingsRow label="Recovery key" detail="Gets you back in if you forget your password.">
            <Button variant="outline" onClick={() => setDialog("recovery")}>
              Create new key
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
                ? `Your bucket only ever holds encrypted files. A copy is cached on ${thisDevice} for offline use.`
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
          <SettingsRow
            label="Backups"
            detail={
              settings.backupDir
                ? `${displayPath(settings.backupDir)} · ${settings.lastBackup ? `last ${backupAge(settings.lastBackup)}` : "none yet"}. Daily while unlocked; the last 10 are kept, encrypted.`
                : "Dated, encrypted copies in a folder you choose. Restore one by connecting to it."
            }
          >
            {settings.backupDir ? (
              <Button variant="outline" onClick={backUpNow} disabled={backingUp}>
                {backingUp ? <Spinner /> : <Archive aria-hidden />}
                Back up now
              </Button>
            ) : (
              <Button variant="outline" onClick={chooseBackupFolder}>
                <Archive aria-hidden />
                Choose folder
              </Button>
            )}
          </SettingsRow>
          <SettingsRow label="Move vault" detail="Copy it to another folder and use that. The old copy stays until you delete it.">
            <Button variant="outline" onClick={moveVault} disabled={moving}>
              {moving ? <Spinner /> : <FolderInput aria-hidden />}
              Move
            </Button>
          </SettingsRow>
          <SettingsRow label="Auto-lock" detail={`Locks when you haven't used dotsafe for a while. Set on ${thisDevice}.`}>
            <Select value={String(settings.autoLockMinutes)} onValueChange={(value) => setAutoLock(Number(value))}>
              <SelectTrigger aria-label="Auto-lock" className="w-40">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {[...new Set([...AUTO_LOCK_MINUTES, settings.autoLockMinutes])]
                  .sort((a, b) => a - b)
                  .map((minutes) => (
                    <SelectItem key={minutes} value={String(minutes)}>
                      {autoLockLabel(minutes)}
                    </SelectItem>
                  ))}
              </SelectContent>
            </Select>
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

const AUTO_LOCK_MINUTES = [1, 5, 15, 30, 60, 240];

function autoLockLabel(minutes: number): string {
  if (minutes % 60 === 0) return `After ${minutes / 60} ${minutes === 60 ? "hour" : "hours"}`;
  return `After ${minutes} ${minutes === 1 ? "minute" : "minutes"}`;
}

function backupAge(at: number): string {
  const hours = Math.round((Date.now() - at) / 3_600_000);
  if (hours < 1) return "just now";
  if (hours < 48) return `${hours} ${hours === 1 ? "hour" : "hours"} ago`;
  return `${Math.round(hours / 24)} days ago`;
}

function syncedLabel(at: number | null): string {
  if (!at) return "Syncs every minute while dotsafe is unlocked.";
  const minutes = Math.round((Date.now() - at) / 60_000);
  if (minutes < 1) return "Synced just now.";
  return `Synced ${new Intl.RelativeTimeFormat(undefined, { numeric: "auto" }).format(-minutes, "minute")}.`;
}
