import { useRef, useState, type FormEvent } from "react";
import { AnimatePresence, motion, useAnimate, useReducedMotion } from "motion/react";
import { Lock } from "lucide-react";
import { api, message } from "@/lib/api";
import { fast } from "@/lib/motion";
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
  const [scope, animate] = useAnimate();
  const reduceMotion = useReducedMotion();
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
      setPassword("");
      setBusy(false);
      input.current?.focus();
      // A short horizontal shake, like the macOS login window.
      if (!reduceMotion) animate(scope.current, { x: [0, -7, 6, -4, 3, 0] }, { duration: 0.36, ease: "easeOut" });
    }
  }

  return (
    <Centered icon={<Lock aria-hidden />} title="dotsafe is locked" subtitle="Enter your master password to unlock.">
      <form onSubmit={submit} className="flex flex-col gap-3">
        <div ref={scope}>
          <Label htmlFor="password" className="sr-only">
            Master password
          </Label>
          <Input
            ref={input}
            id="password"
            type="password"
            autoComplete="current-password"
            autoFocus
            placeholder="Master password"
            className="h-10"
            value={password}
            aria-invalid={!!error}
            aria-describedby={error ? "unlock-error" : undefined}
            onChange={(e) => setPassword(e.target.value)}
          />
        </div>
        <AnimatePresence initial={false}>
          {error && (
            <motion.p
              id="unlock-error"
              role="alert"
              className="text-xs text-destructive"
              initial={{ opacity: 0, y: -2 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0 }}
              transition={fast}
            >
              {error}
            </motion.p>
          )}
        </AnimatePresence>
        <div className="flex items-center gap-3 pt-2">
          <p title={vaultPath ?? undefined} className="min-w-0 flex-1 truncate text-xs text-muted-foreground">
            {vaultPath && displayPath(vaultPath)}
          </p>
          <Button type="submit" size="lg" className="px-4" disabled={busy || !password}>
            {busy && <Spinner />}
            Unlock
          </Button>
        </div>
      </form>
    </Centered>
  );
}
