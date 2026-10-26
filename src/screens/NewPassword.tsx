import { useState, type FormEvent } from "react";
import { KeyRound } from "lucide-react";
import { api, message } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Spinner } from "@/components/ui/spinner";
import { Centered } from "@/components/Centered";
import { Field } from "@/components/Field";
import { SurfaceFooter } from "@/components/Surface";

const MIN_LENGTH = 8;

/** After unlocking with the recovery key, the user picks a new master password
 * before they can use the vault. */
export function NewPassword({ onDone, onLock }: { onDone: () => void; onLock: () => void }) {
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function submit(e: FormEvent) {
    e.preventDefault();
    setError(null);
    if (password.length < MIN_LENGTH) return setError(`Use at least ${MIN_LENGTH} characters.`);
    if (password !== confirm) return setError("Passwords don't match.");
    setBusy(true);
    try {
      await api.changePassword(null, password);
      onDone();
    } catch (err) {
      setError(message(err));
      setBusy(false);
    }
  }

  return (
    <Centered
      icon={<KeyRound aria-hidden />}
      title="Choose a new master password"
      subtitle="You unlocked with your recovery key. Set a new password to keep using dotsafe."
    >
      <form onSubmit={submit} className="flex flex-col gap-4">
        <Field label="New master password" htmlFor="new-password" error={error}>
          <Input
            id="new-password"
            type="password"
            autoComplete="new-password"
            autoFocus
            value={password}
            aria-invalid={!!error}
            onChange={(e) => setPassword(e.target.value)}
            className="h-9"
          />
        </Field>
        <Field
          label="Confirm password"
          htmlFor="confirm-password"
          hint="This changes it on every device synced to this vault. Your recovery key keeps working."
        >
          <Input
            id="confirm-password"
            type="password"
            autoComplete="new-password"
            value={confirm}
            onChange={(e) => setConfirm(e.target.value)}
            className="h-9"
          />
        </Field>
        <SurfaceFooter className="pt-1">
          <Button type="button" variant="ghost" onClick={onLock}>
            Lock instead
          </Button>
          <Button type="submit" size="lg" className="px-4" disabled={busy || !password}>
            {busy && <Spinner />}
            Set password
          </Button>
        </SurfaceFooter>
      </form>
    </Centered>
  );
}
