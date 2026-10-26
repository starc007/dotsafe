import { useRef, useState, type FormEvent } from "react";
import { AnimatePresence, motion, useAnimate, useReducedMotion } from "motion/react";
import { KeyRound, Lock } from "lucide-react";
import { api, message } from "@/lib/api";
import { fast } from "@/lib/motion";
import { displayPath } from "@/lib/paths";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Spinner } from "@/components/ui/spinner";
import { Centered } from "@/components/Centered";

type Mode = "password" | "recovery";

export function Unlock({ vaultPath, onUnlocked }: { vaultPath: string | null; onUnlocked: () => void }) {
  const [mode, setMode] = useState<Mode>("password");
  const [value, setValue] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [scope, animate] = useAnimate();
  const reduceMotion = useReducedMotion();
  const input = useRef<HTMLInputElement>(null);
  const recovering = mode === "recovery";

  function switchMode(next: Mode) {
    setMode(next);
    setValue("");
    setError(null);
    requestAnimationFrame(() => input.current?.focus());
  }

  async function submit(e: FormEvent) {
    e.preventDefault();
    if (!value.trim()) return;
    setBusy(true);
    setError(null);
    try {
      if (recovering) await api.unlockWithRecovery(value);
      else await api.unlock(value);
      onUnlocked();
    } catch (err) {
      setError(message(err));
      // A mistyped recovery key is long; keep it so it can be corrected.
      if (!recovering) setValue("");
      setBusy(false);
      input.current?.focus();
      // A short horizontal shake, like the macOS login window.
      if (!reduceMotion) animate(scope.current, { x: [0, -7, 6, -4, 3, 0] }, { duration: 0.36, ease: "easeOut" });
    }
  }

  return (
    <Centered
      icon={recovering ? <KeyRound aria-hidden /> : <Lock aria-hidden />}
      title={recovering ? "Use your recovery key" : "dotsafe is locked"}
      subtitle={
        recovering
          ? "Enter the key you saved when you set up dotsafe. You'll choose a new password next."
          : "Enter your master password to unlock."
      }
    >
      <form onSubmit={submit} className="flex flex-col gap-3">
        <div ref={scope}>
          <Label htmlFor="unlock-input" className="sr-only">
            {recovering ? "Recovery key" : "Master password"}
          </Label>
          <Input
            // Remounted per mode so the browser treats them as separate fields.
            key={mode}
            ref={input}
            id="unlock-input"
            type={recovering ? "text" : "password"}
            autoComplete={recovering ? "off" : "current-password"}
            autoFocus
            placeholder={recovering ? "XXXX-XXXX-XXXX-…" : "Master password"}
            className={recovering ? "h-10 font-mono text-[13px] uppercase tracking-wider md:text-[13px]" : "h-10"}
            value={value}
            aria-invalid={!!error}
            aria-describedby={error ? "unlock-error" : recovering ? "recovery-hint" : undefined}
            onChange={(e) => setValue(e.target.value)}
          />
        </div>
        <AnimatePresence initial={false} mode="popLayout">
          {error ? (
            <motion.p
              key="error"
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
          ) : recovering ? (
            <motion.p
              key="hint"
              id="recovery-hint"
              className="text-xs text-muted-foreground"
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              transition={fast}
            >
              Dashes, spaces and capitals don't matter.
            </motion.p>
          ) : null}
        </AnimatePresence>
        <div className="flex items-center gap-3 pt-2">
          <button
            type="button"
            onClick={() => switchMode(recovering ? "password" : "recovery")}
            className="min-w-0 flex-1 truncate text-left text-xs text-muted-foreground underline-offset-2 hover:text-foreground hover:underline focus-visible:text-foreground focus-visible:underline focus-visible:outline-none"
          >
            {recovering ? "Use my password instead" : "Forgot your password?"}
          </button>
          <Button type="submit" size="lg" className="px-4" disabled={busy || !value.trim()}>
            {busy && <Spinner />}
            Unlock
          </Button>
        </div>
        {vaultPath && (
          <p title={vaultPath} className="truncate text-center text-xs text-muted-foreground/80">
            {displayPath(vaultPath)}
          </p>
        )}
      </form>
    </Centered>
  );
}
