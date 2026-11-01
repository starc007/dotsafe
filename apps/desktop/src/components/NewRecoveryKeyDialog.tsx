import { useState, type FormEvent } from "react";
import { toast } from "sonner";
import { LifeBuoy } from "lucide-react";
import { api, message } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { DialogClose } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Spinner } from "@/components/ui/spinner";
import { Field } from "@/components/Field";
import { RecoveryKeyPanel } from "@/components/RecoveryKeyPanel";
import { SurfaceFooter } from "@/components/Surface";
import { SurfaceDialog } from "@/components/SurfaceDialog";

/** Replaces the recovery key: confirm the password, then show the new key once. */
export function NewRecoveryKeyDialog({ open, onOpenChange }: { open: boolean; onOpenChange: (open: boolean) => void }) {
  const [recoveryKey, setRecoveryKey] = useState<string | null>(null);

  function setOpen(next: boolean) {
    // Once the new key exists, closing without confirming would lose it.
    if (!next && recoveryKey) {
      toast("Save the new key first", { description: "Tick the box once it's stored somewhere safe." });
      return;
    }
    if (!next) setRecoveryKey(null);
    onOpenChange(next);
  }

  return (
    <SurfaceDialog
      open={open}
      onOpenChange={setOpen}
      icon={<LifeBuoy aria-hidden />}
      title={recoveryKey ? "Save your new recovery key" : "Create a new recovery key"}
      description={recoveryKey ? "Your old recovery key no longer works." : "Your current recovery key will stop working."}
    >
      {recoveryKey ? (
        <RecoveryKeyPanel
          recoveryKey={recoveryKey}
          doneLabel="Done"
          onDone={() => {
            setRecoveryKey(null);
            onOpenChange(false);
            toast.success("New recovery key saved");
          }}
        />
      ) : (
        <ConfirmPassword onCreated={setRecoveryKey} />
      )}
    </SurfaceDialog>
  );
}

function ConfirmPassword({ onCreated }: { onCreated: (key: string) => void }) {
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function submit(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      onCreated(await api.resetRecoveryKey(password));
    } catch (err) {
      setError(message(err));
      setBusy(false);
    }
  }

  return (
    <form onSubmit={submit} className="flex flex-col gap-4">
      <Field label="Master password" htmlFor="recovery-password" error={error} hint="To make sure it's you.">
        <Input
          id="recovery-password"
          type="password"
          autoComplete="current-password"
          autoFocus
          value={password}
          aria-invalid={!!error}
          onChange={(e) => setPassword(e.target.value)}
          className="h-9"
        />
      </Field>
      <SurfaceFooter className="pt-1">
        <DialogClose asChild>
          <Button type="button" variant="ghost">
            Cancel
          </Button>
        </DialogClose>
        <Button type="submit" disabled={busy || !password}>
          {busy && <Spinner />}
          Create new key
        </Button>
      </SurfaceFooter>
    </form>
  );
}
