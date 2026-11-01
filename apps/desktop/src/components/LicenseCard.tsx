import { useState, type FormEvent, type ReactNode } from "react";
import { toast } from "sonner";
import { BadgeCheck, Check, Copy, Eye, EyeOff, KeyRound, ShieldAlert, ShieldX } from "lucide-react";
import { api, ApiError, message, type License } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Spinner } from "@/components/ui/spinner";
import { ActionSwapIcon, ActionSwapText } from "@/components/motion/action-swap";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { Field } from "@/components/Field";
import { IconButton } from "@/components/IconButton";
import { Surface, SurfaceBody, SurfaceDescription, SurfaceFooter, SurfaceHeader, SurfaceTitle } from "@/components/Surface";

/** Shows the license activated on this Mac, or a form to activate one. */
export function LicenseCard({ license, onChanged }: { license: License | null; onChanged: () => void }) {
  return (
    <Surface aria-labelledby="license-title">
      <SurfaceHeader icon={license ? STATUS[license.status].icon : <KeyRound aria-hidden />}>
        <SurfaceTitle id="license-title">License key</SurfaceTitle>
        <SurfaceDescription>{license ? STATUS[license.status].describe(license) : "Not activated yet"}</SurfaceDescription>
      </SurfaceHeader>
      <SurfaceBody>
        {license ? <SavedLicense license={license} onChanged={onChanged} /> : <ActivateForm onActivated={onChanged} />}
      </SurfaceBody>
    </Surface>
  );
}

const STATUS: Record<License["status"], { icon: ReactNode; describe: (l: License) => string; notice?: string }> = {
  active: {
    icon: <BadgeCheck aria-hidden className="text-primary" />,
    describe: (l) => `Active on ${l.deviceName}`,
  },
  unverified: {
    icon: <ShieldAlert aria-hidden />,
    describe: (l) => `Not verified since ${formatDate(l.checkedAt)}`,
    notice: "dotsafe hasn't reached the license server in a while. Everything keeps working; connect to the internet and check again when you can.",
  },
  revoked: {
    icon: <ShieldX aria-hidden className="text-destructive" />,
    describe: () => "No longer valid",
    notice: "The license server says this key is no longer valid. It may have been refunded or disabled. Contact support if that's a mistake.",
  },
};

function SavedLicense({ license, onChanged }: { license: License; onChanged: () => void }) {
  const [revealed, setRevealed] = useState(false);
  const [copied, setCopied] = useState(false);
  // "offline": deactivating failed for lack of a connection; offer to remove anyway.
  const [confirming, setConfirming] = useState<false | "deactivate" | "offline">(false);
  const [busy, setBusy] = useState(false);
  const [checking, setChecking] = useState(false);
  const notice = STATUS[license.status].notice;

  async function copy() {
    await navigator.clipboard.writeText(license.key);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1500);
  }

  async function check() {
    setChecking(true);
    try {
      const updated = await api.checkLicense();
      if (updated?.status === "active") toast.success("License verified");
      else if (updated?.status === "revoked") toast.error("This license is no longer valid");
      onChanged();
    } catch (e) {
      toast.error(message(e));
    } finally {
      setChecking(false);
    }
  }

  async function remove() {
    setBusy(true);
    try {
      await api.removeLicense(confirming === "offline");
      toast("License removed from this Mac");
      setConfirming(false);
      onChanged();
    } catch (e) {
      if (e instanceof ApiError && e.kind === "offline") setConfirming("offline");
      else {
        toast.error(message(e));
        setConfirming(false);
      }
    } finally {
      setBusy(false);
    }
  }

  return (
    <>
      {notice && (
        <p role="status" className="mb-4 rounded-xl bg-muted/60 px-3.5 py-2.5 text-[13px] text-muted-foreground">
          {notice}
        </p>
      )}
      <p id="license-key-label" className="mb-1.5 text-xs font-medium text-muted-foreground">
        License key
      </p>
      <div className="flex items-center gap-1 rounded-full bg-muted/60 py-1 pr-1 pl-3.5">
        <span aria-labelledby="license-key-label" className="min-w-0 flex-1 font-mono text-[12.5px]">
          <ActionSwapText value={revealed ? "key" : "mask"} animation="roll">
            <span className={revealed ? "selectable" : "tracking-wider text-muted-foreground"}>
              {revealed ? license.key : maskLicense(license.key)}
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
      <dl className="mt-3 grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-[13px]">
        <dt className="text-muted-foreground">Activated</dt>
        <dd>{formatDate(license.activatedAt)}</dd>
        <dt className="text-muted-foreground">Last checked</dt>
        <dd>{formatDate(license.checkedAt)}</dd>
      </dl>
      <SurfaceFooter>
        <Button variant="ghost" className="text-destructive hover:text-destructive" onClick={() => setConfirming("deactivate")}>
          Deactivate
        </Button>
        <Button variant="secondary" disabled={checking} onClick={check}>
          {checking && <Spinner />}
          Check now
        </Button>
      </SurfaceFooter>
      <ConfirmDialog
        open={!!confirming}
        busy={busy}
        title={confirming === "offline" ? "Remove without deactivating?" : "Deactivate this Mac?"}
        description={
          confirming === "offline"
            ? "dotsafe couldn't reach the license server, so this Mac's activation can't be freed right now. If you remove the key anyway, it keeps counting toward your device limit."
            : "This frees the activation so you can use the key on another device. Your vault and secrets aren't affected."
        }
        confirmLabel={confirming === "offline" ? "Remove anyway" : "Deactivate"}
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

const dateFormat = new Intl.DateTimeFormat(undefined, { dateStyle: "medium" });
function formatDate(ms: number) {
  return dateFormat.format(new Date(ms));
}

/** Keeps the start and end so a key can be recognized without showing it all. */
function maskLicense(key: string) {
  if (key.length <= 8) return "•".repeat(key.length);
  return `${key.slice(0, 4)}${"•".repeat(Math.min(12, key.length - 8))}${key.slice(-4)}`;
}
