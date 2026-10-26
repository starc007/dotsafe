import { useCallback, useEffect, useState, type FormEvent } from "react";
import { toast } from "sonner";
import { Fingerprint, Laptop, Plus } from "lucide-react";
import { api, message, type DevicesView, type DeviceView } from "@/lib/api";
import { daysAgo } from "@/lib/time";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Skeleton } from "@/components/ui/skeleton";
import { Spinner } from "@/components/ui/spinner";
import { Field } from "@/components/Field";
import { RecoveryKeyPanel } from "@/components/RecoveryKeyPanel";
import { SurfaceDialog } from "@/components/SurfaceDialog";
import { Surface, SurfaceBody, SurfaceDescription, SurfaceFooter, SurfaceHeader, SurfaceTitle } from "@/components/Surface";

/** Touch ID unlock for this Mac, the devices that can open the vault,
 * adding one with a code, and removing one. */
export function DeviceSettings() {
  const [view, setView] = useState<DevicesView | null>(null);
  const [adding, setAdding] = useState(false);
  const [removing, setRemoving] = useState<DeviceView | null>(null);
  const [busy, setBusy] = useState(false);

  const load = useCallback(async () => {
    try {
      setView(await api.listDevices());
    } catch (e) {
      toast.error(message(e));
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  async function setTouchId(enabled: boolean) {
    setBusy(true);
    try {
      await api.setTouchIdUnlock(enabled);
      toast(enabled ? "This Mac unlocks with Touch ID" : "Touch ID unlock is off on this Mac");
    } catch (e) {
      toast(message(e));
    } finally {
      setBusy(false);
      load();
    }
  }

  const others = view?.devices.filter((d) => !d.thisDevice) ?? [];
  const stale = view?.devices.find((d) => d.thisDevice)?.stale;

  return (
    <>
      <Surface aria-labelledby="touch-id-title">
        <SurfaceHeader icon={<Fingerprint aria-hidden />}>
          <SurfaceTitle id="touch-id-title">Touch ID</SurfaceTitle>
          <SurfaceDescription>Unlock this Mac without typing your password.</SurfaceDescription>
        </SurfaceHeader>
        <SurfaceBody>
          {view === null ? (
            <Skeleton aria-hidden className="h-5 w-1/2" />
          ) : view.touchIdAvailable ? (
            <div className="flex items-start gap-3">
              <Checkbox
                id="touch-id-unlock"
                checked={view.touchIdUnlock}
                disabled={busy}
                onCheckedChange={(checked) => setTouchId(checked === true)}
                className="mt-0.5"
              />
              <div className="flex flex-col gap-0.5">
                <Label htmlFor="touch-id-unlock" className="font-medium">
                  Unlock with Touch ID
                </Label>
                <p className="text-xs text-muted-foreground">
                  dotsafe still asks for your password every two weeks, so you don't forget it.
                  {stale && " This Mac needs your password once, because a device was removed."}
                </p>
              </div>
            </div>
          ) : (
            <p className="text-muted-foreground">Touch ID unlock needs a Mac with Touch ID or a login password.</p>
          )}
        </SurfaceBody>
      </Surface>

      <Surface aria-labelledby="devices-title">
        <SurfaceHeader
          icon={<Laptop aria-hidden />}
          action={
            <Button variant="outline" className="bg-card" onClick={() => setAdding(true)}>
              <Plus aria-hidden />
              Add a device
            </Button>
          }
        >
          <SurfaceTitle id="devices-title">Other devices</SurfaceTitle>
          <SurfaceDescription>Devices that joined with a code or turned on Touch ID.</SurfaceDescription>
        </SurfaceHeader>
        <SurfaceBody className={others.length ? "p-0" : undefined}>
          {view === null ? (
            <Skeleton aria-hidden className="h-5 w-2/3" />
          ) : others.length === 0 ? (
            <p className="text-muted-foreground">
              None yet. Devices that only use the password aren't listed; change the password to shut those out.
            </p>
          ) : (
            <ul className="divide-y">
              {others.map((d) => (
                <li key={d.deviceId} className="flex items-center gap-3 px-4 py-2.5">
                  <Laptop aria-hidden className="size-4 shrink-0 text-muted-foreground" />
                  <span className="min-w-0 flex-1">
                    <span className="block truncate font-medium">{d.name}</span>
                    <span className="block truncate text-xs text-muted-foreground">
                      Added {daysAgo(d.added)}
                      {d.stale && " · needs the password once"}
                    </span>
                  </span>
                  <Button
                    variant="ghost"
                    className="text-destructive hover:text-destructive"
                    onClick={() => setRemoving(d)}
                  >
                    Remove…
                  </Button>
                </li>
              ))}
            </ul>
          )}
        </SurfaceBody>
      </Surface>

      <AddDeviceDialog open={adding} onOpenChange={setAdding} />
      <RemoveDeviceDialog device={removing} onClose={() => setRemoving(null)} onRemoved={load} />
    </>
  );
}

function AddDeviceDialog({ open, onOpenChange }: { open: boolean; onOpenChange: (open: boolean) => void }) {
  const [code, setCode] = useState<{ code: string; expiresAt: number } | null>(null);
  const [now, setNow] = useState(Date.now());

  useEffect(() => {
    if (!open) return;
    setCode(null);
    api
      .createEnrollmentCode()
      .then(setCode)
      .catch((e) => {
        toast.error(message(e));
        onOpenChange(false);
      });
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [open, onOpenChange]);

  const left = code ? Math.max(0, Math.round((code.expiresAt - now) / 1000)) : 0;
  return (
    <SurfaceDialog
      open={open}
      onOpenChange={onOpenChange}
      icon={<Plus aria-hidden />}
      title="Add a device"
      description="On the new Mac, set up dotsafe, choose this vault's folder or bucket, and enter this code instead of the password."
    >
      {!code ? (
        <Skeleton aria-hidden className="h-24 rounded-2xl" />
      ) : (
        <div className="flex flex-col gap-3">
          <div
            aria-label="Code"
            className="selectable grid grid-cols-4 gap-x-3 gap-y-2 rounded-2xl bg-muted/60 p-4 text-center font-mono text-[15px] tracking-wider"
          >
            {code.code.split("-").map((group, i) => (
              <span key={i}>{group}</span>
            ))}
          </div>
          <p className="text-xs text-muted-foreground">
            {left > 0
              ? `Works once, for ${Math.floor(left / 60)}:${String(left % 60).padStart(2, "0")} more. Anyone with it can join, so only type it yourself.`
              : "This code has expired. Close and add a device again for a new one."}
          </p>
          <SurfaceFooter className="pt-1">
            <Button onClick={() => onOpenChange(false)}>Done</Button>
          </SurfaceFooter>
        </div>
      )}
    </SurfaceDialog>
  );
}

function RemoveDeviceDialog({
  device,
  onClose,
  onRemoved,
}: {
  device: DeviceView | null;
  onClose: () => void;
  onRemoved: () => void;
}) {
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [recoveryKey, setRecoveryKey] = useState<string | null>(null);
  const [shown, setShown] = useState<DeviceView | null>(null);

  useEffect(() => {
    if (device) {
      setShown(device);
      setPassword("");
      setError(null);
      setRecoveryKey(null);
    }
  }, [device]);

  async function submit(e: FormEvent) {
    e.preventDefault();
    if (!device) return;
    setBusy(true);
    setError(null);
    try {
      setRecoveryKey(await api.revokeDevice(device.deviceId, password));
      onRemoved();
    } catch (err) {
      setError(message(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <SurfaceDialog
      open={device !== null}
      onOpenChange={(open) => !open && !busy && onClose()}
      icon={<Laptop aria-hidden />}
      title={recoveryKey ? "Save your new recovery key" : `Remove ${shown?.name ?? "device"}?`}
      description={
        recoveryKey
          ? "The vault has a new key, so your old recovery key no longer works. This is the only time this one is shown."
          : "dotsafe switches the vault to a new key and re-encrypts everything, so this device can't read anything from now on."
      }
    >
      {recoveryKey ? (
        <RecoveryKeyPanel recoveryKey={recoveryKey} doneLabel="Done" onDone={onClose} />
      ) : (
        <form onSubmit={submit} className="flex flex-col gap-4">
          <ul className="flex list-disc flex-col gap-1 pl-5 text-[13px] text-muted-foreground">
            <li>You'll get a new recovery key; the old one stops working.</li>
            <li>Other devices with Touch ID will ask for the password once.</li>
            <li>It can still read what it already had. Rotate the secrets it could see, most important first.</li>
          </ul>
          <Field label="Your master password" htmlFor="revoke-password" error={error}>
            <Input
              id="revoke-password"
              type="password"
              autoComplete="current-password"
              autoFocus
              className="h-9"
              value={password}
              aria-invalid={!!error}
              onChange={(e) => setPassword(e.target.value)}
            />
          </Field>
          <SurfaceFooter className="pt-1">
            <Button type="button" variant="ghost" onClick={onClose} disabled={busy}>
              Cancel
            </Button>
            <Button type="submit" variant="destructive" disabled={busy || !password}>
              {busy && <Spinner />}
              Remove and re-key
            </Button>
          </SurfaceFooter>
        </form>
      )}
    </SurfaceDialog>
  );
}
