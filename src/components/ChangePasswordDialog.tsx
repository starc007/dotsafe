import { useState, type FormEvent } from "react";
import { toast } from "sonner";
import { KeyRound } from "lucide-react";
import { api, message } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { DialogClose } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Spinner } from "@/components/ui/spinner";
import { Field } from "@/components/Field";
import { SurfaceFooter } from "@/components/Surface";
import { SurfaceDialog } from "@/components/SurfaceDialog";

const MIN_LENGTH = 8;

/** Changes the master password; asks for the current one first. */
export function ChangePasswordDialog({ open, onOpenChange }: { open: boolean; onOpenChange: (open: boolean) => void }) {
  return (
    <SurfaceDialog
      open={open}
      onOpenChange={onOpenChange}
      icon={<KeyRound aria-hidden />}
      title="Change master password"
      description="Changes it on every device synced to this vault."
    >
      <ChangePasswordForm onDone={() => onOpenChange(false)} />
    </SurfaceDialog>
  );
}

function ChangePasswordForm({ onDone }: { onDone: () => void }) {
  const [current, setCurrent] = useState("");
  const [next, setNext] = useState("");
  const [confirm, setConfirm] = useState("");
  const [error, setError] = useState<{ field: "current" | "next"; text: string } | null>(null);
  const [busy, setBusy] = useState(false);

  async function submit(e: FormEvent) {
    e.preventDefault();
    setError(null);
    if (next.length < MIN_LENGTH) return setError({ field: "next", text: `Use at least ${MIN_LENGTH} characters.` });
    if (next !== confirm) return setError({ field: "next", text: "New passwords don't match." });
    if (next === current) return setError({ field: "next", text: "Choose a password different from the current one." });
    setBusy(true);
    try {
      await api.changePassword(current, next);
      toast.success("Master password changed");
      onDone();
    } catch (err) {
      const text = message(err);
      setError({ field: /current/i.test(text) ? "current" : "next", text });
      setBusy(false);
    }
  }

  return (
    <form onSubmit={submit} className="flex flex-col gap-4">
      <Field label="Current password" htmlFor="current-password" error={error?.field === "current" ? error.text : null}>
        <Input
          id="current-password"
          type="password"
          autoComplete="current-password"
          autoFocus
          value={current}
          aria-invalid={error?.field === "current"}
          onChange={(e) => setCurrent(e.target.value)}
          className="h-9"
        />
      </Field>
      <Field label="New password" htmlFor="new-password" error={error?.field === "next" ? error.text : null}>
        <Input
          id="new-password"
          type="password"
          autoComplete="new-password"
          value={next}
          aria-invalid={error?.field === "next"}
          onChange={(e) => setNext(e.target.value)}
          className="h-9"
        />
      </Field>
      <Field label="Confirm new password" htmlFor="confirm-password" hint="Your recovery key keeps working.">
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
        <DialogClose asChild>
          <Button type="button" variant="ghost">
            Cancel
          </Button>
        </DialogClose>
        <Button type="submit" disabled={busy || !current || !next}>
          {busy && <Spinner />}
          Change password
        </Button>
      </SurfaceFooter>
    </form>
  );
}
