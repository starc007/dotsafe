import { useRef, useState, type FormEvent } from "react";
import { api, message } from "../lib/api";
import { displayPath } from "../lib/paths";
import { Button, Input } from "../components/ui";
import { Centered } from "../components/Centered";

export function Unlock({ vaultPath, onUnlocked }: { vaultPath: string | null; onUnlocked: () => void }) {
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [attempt, setAttempt] = useState(0);
  const input = useRef<HTMLInputElement>(null);

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
      input.current?.focus();
    }
  }

  return (
    <Centered title="dotsafe is locked" subtitle="Enter your master password to unlock.">
      <form onSubmit={submit} className="flex flex-col gap-3">
        <div key={attempt} className={attempt > 0 ? "animate-shake" : undefined}>
          <label htmlFor="password" className="sr-only">
            Master password
          </label>
          <Input
            ref={input}
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
          <p id="unlock-error" role="alert" className="text-center text-xs text-danger">
            {error}
          </p>
        )}
        <Button type="submit" variant="primary" className="h-9 w-full" loading={busy} disabled={!password}>
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
