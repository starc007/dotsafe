import { useState, type FormEvent } from "react";
import { toast } from "sonner";
import { BadgeCheck, Check, Copy, Eye, EyeOff, KeyRound } from "lucide-react";
import { api, message } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Spinner } from "@/components/ui/spinner";
import { ActionSwapIcon, ActionSwapText } from "@/components/motion/action-swap";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { Field } from "@/components/Field";
import { IconButton } from "@/components/IconButton";
import { Surface, SurfaceBody, SurfaceDescription, SurfaceFooter, SurfaceHeader, SurfaceTitle } from "@/components/Surface";

/** Shows the license key saved on this Mac, or a form to enter one. */
export function LicenseCard({ licenseKey, onChanged }: { licenseKey: string | null; onChanged: () => void }) {
  return (
    <Surface aria-labelledby="license-title">
      <SurfaceHeader icon={licenseKey ? <BadgeCheck aria-hidden className="text-primary" /> : <KeyRound aria-hidden />}>
        <SurfaceTitle id="license-title">License</SurfaceTitle>
        <SurfaceDescription>{licenseKey ? "Activated on this Mac" : "Not activated yet"}</SurfaceDescription>
      </SurfaceHeader>
      <SurfaceBody>
        {licenseKey ? <SavedLicense licenseKey={licenseKey} onRemoved={onChanged} /> : <ActivateForm onActivated={onChanged} />}
      </SurfaceBody>
    </Surface>
  );
}

function SavedLicense({ licenseKey, onRemoved }: { licenseKey: string; onRemoved: () => void }) {
  const [revealed, setRevealed] = useState(false);
  const [copied, setCopied] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [busy, setBusy] = useState(false);

  async function copy() {
    await navigator.clipboard.writeText(licenseKey);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1500);
  }

  async function remove() {
    setBusy(true);
    try {
      await api.removeLicense();
      toast("License removed from this Mac");
      onRemoved();
    } catch (e) {
      toast.error(message(e));
    } finally {
      setBusy(false);
      setConfirming(false);
    }
  }

  return (
    <>
      <p id="license-key-label" className="mb-1.5 text-xs font-medium text-muted-foreground">
        License key
      </p>
      <div className="flex items-center gap-1 rounded-full bg-muted/60 py-1 pr-1 pl-3.5">
        <span aria-labelledby="license-key-label" className="min-w-0 flex-1 font-mono text-[12.5px]">
          <ActionSwapText value={revealed ? "key" : "mask"} animation="roll">
            <span className={revealed ? "selectable" : "tracking-wider text-muted-foreground"}>
              {revealed ? licenseKey : maskLicense(licenseKey)}
            </span>
          </ActionSwapText>
        </span>
        <IconButton label={revealed ? "Hide license key" : "Show license key"} onClick={() => setRevealed((r) => !r)}>
          <ActionSwapIcon value={revealed ? "hide" : "show"} animation="blur" className="size-4">
            {revealed ? <EyeOff aria-hidden /> : <Eye aria-hidden />}
          </ActionSwapIcon>
        </IconButton>
        <IconButton label="Copy license key" onClick={copy}>
          <ActionSwapIcon value={copied ? "copied" : "copy"} animation="blur" className="size-4">
            {copied ? <Check aria-hidden /> : <Copy aria-hidden />}
          </ActionSwapIcon>
        </IconButton>
      </div>
      <SurfaceFooter>
        <Button variant="ghost" className="text-destructive hover:text-destructive" onClick={() => setConfirming(true)}>
          Remove license
        </Button>
      </SurfaceFooter>
      <ConfirmDialog
        open={confirming}
        busy={busy}
        title="Remove the license?"
        description="dotsafe will forget this key on this Mac. Your vault and secrets aren't affected, and you can enter the key again any time."
        confirmLabel="Remove"
        onConfirm={remove}
        onCancel={() => setConfirming(false)}
      />
    </>
  );
}

function ActivateForm({ onActivated }: { onActivated: () => void }) {
  const [key, setKey] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function submit(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      await api.activateLicense(key);
      toast.success("License activated");
      onActivated();
    } catch (err) {
      setError(message(err));
      setBusy(false);
    }
  }

  return (
    <form onSubmit={submit}>
      <Field label="License key" htmlFor="license-key" error={error} hint="You'll find it in your purchase email.">
        <Input
          id="license-key"
          placeholder="DOTSAFE-XXXX-XXXX-XXXX"
          autoComplete="off"
          value={key}
          aria-invalid={!!error}
          onChange={(e) => setKey(e.target.value)}
          className="h-9 font-mono text-[12.5px] md:text-[12.5px]"
        />
      </Field>
      <SurfaceFooter>
        <Button type="submit" disabled={busy || !key.trim()}>
          {busy && <Spinner />}
          Activate
        </Button>
      </SurfaceFooter>
    </form>
  );
}

/** Keeps the start and end so a key can be recognized without showing it all. */
function maskLicense(key: string) {
  if (key.length <= 8) return "•".repeat(key.length);
  return `${key.slice(0, 4)}${"•".repeat(Math.min(12, key.length - 8))}${key.slice(-4)}`;
}
