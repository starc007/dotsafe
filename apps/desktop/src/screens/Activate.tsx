import { useState, type FormEvent } from "react";
import { ArrowUpRight, KeyRound } from "lucide-react";
import { api, message } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Spinner } from "@/components/ui/spinner";
import { Centered } from "@/components/Centered";

/** The first screen on a Mac without a license: dotsafe is paid, so a key comes before setup. */
export function Activate({ onActivated }: { onActivated: () => void }) {
  const [key, setKey] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function submit(e: FormEvent) {
    e.preventDefault();
    if (!key.trim()) return;
    setBusy(true);
    setError(null);
    try {
      await api.activateLicense(key);
      onActivated();
    } catch (err) {
      setError(message(err));
      setBusy(false);
    }
  }

  async function buy() {
    try {
      await api.openPurchasePage();
    } catch (err) {
      setError(message(err));
    }
  }

  return (
    <Centered
      icon={<KeyRound aria-hidden />}
      title="Activate dotsafe"
      subtitle="Enter the license key from your purchase email."
    >
      <form onSubmit={submit} className="flex flex-col gap-3">
        <Label htmlFor="license-key" className="sr-only">
          License key
        </Label>
        <Input
          id="license-key"
          autoFocus
          autoComplete="off"
          spellCheck={false}
          placeholder="DOTSAFE-XXXX-XXXX-XXXX"
          className="h-10 font-mono text-[13px] md:text-[13px]"
          value={key}
          aria-invalid={!!error}
          aria-describedby={error ? "license-error" : undefined}
          onChange={(e) => setKey(e.target.value)}
        />
        {error && (
          <p id="license-error" role="alert" className="text-[13px] text-destructive">
            {error}
          </p>
        )}
        <Button type="submit" size="lg" disabled={busy || !key.trim()}>
          {busy && <Spinner />}
          Activate
        </Button>
        <Button type="button" variant="ghost" onClick={buy}>
          Buy a license
          <ArrowUpRight aria-hidden />
        </Button>
      </form>
    </Centered>
  );
}
