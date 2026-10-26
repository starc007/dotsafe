import { useState, type FormEvent } from "react";
import { api, message } from "@/lib/api";
import { displayPath } from "@/lib/paths";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Spinner } from "@/components/ui/spinner";
import { Centered } from "@/components/Centered";

export function Unlock({ vaultPath, onUnlocked }: { vaultPath: string | null; onUnlocked: () => void }) {
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [attempt, setAttempt] = useState(0);

  async function submit(e: FormEvent) {
    e.preventDefault();
    if (!password) return;
    setBusy(true);
    setError(null);
    try {
      await api.unlock(password);
      onUnlocked();
    } catch (err) {
      setError(message(err));
      setAttempt((n) => n + 1);
      setPassword("");
      setBusy(false);
    }
  }

  return (
    <Centered title="dotsafe is locked" subtitle="Enter your master password to unlock.">
      <form onSubmit={submit} className="flex flex-col gap-3">
        {/* Re-keyed per failed attempt so the shake replays; autoFocus refocuses the new input. */}
        <div key={attempt} className={attempt > 0 ? "motion-safe:animate-shake" : undefined}>
          <Label htmlFor="password" className="sr-only">
            Master password
          </Label>
          <Input
            id="password"
            type="password"
            autoComplete="current-password"
            autoFocus
            placeholder="Master password"
            className="h-9"
            value={password}
            aria-invalid={!!error}
            aria-describedby={error ? "unlock-error" : undefined}
            onChange={(e) => setPassword(e.target.value)}
          />
        </div>
        {error && (
          <p id="unlock-error" role="alert" className="text-center text-xs text-destructive">
            {error}
          </p>
        )}
        <Button type="submit" size="lg" className="w-full" disabled={busy || !password}>
          {busy && <Spinner />}
          Unlock
        </Button>
        {vaultPath && (
          <p title={vaultPath} className="mt-2 truncate text-center text-xs text-muted-foreground">
            {displayPath(vaultPath)}
          </p>
        )}
      </form>
    </Centered>
  );
}
