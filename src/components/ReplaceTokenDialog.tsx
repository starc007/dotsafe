import { useEffect, useState, type FormEvent } from "react";
import { toast } from "sonner";
import { KeyRound } from "lucide-react";
import { api, message, type ConnectionView } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Spinner } from "@/components/ui/spinner";
import { Field } from "@/components/Field";
import { SurfaceDialog } from "@/components/SurfaceDialog";
import { SurfaceFooter } from "@/components/Surface";

/** Gives a connection a new admin token, after it was rotated or revoked at
 * the service. */
export function ReplaceTokenDialog({
  connection,
  onClose,
}: {
  connection: ConnectionView | null;
  onClose: () => void;
}) {
  const [token, setToken] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [shown, setShown] = useState<ConnectionView | null>(null);

  useEffect(() => {
    if (!connection) return;
    setShown(connection);
    setToken("");
    setError(null);
  }, [connection]);

  async function submit(e: FormEvent) {
    e.preventDefault();
    if (!connection) return;
    setBusy(true);
    setError(null);
    try {
      await api.replaceConnectionToken(connection.id, token);
      toast.success(`${connection.name} uses the new token`);
      onClose();
    } catch (err) {
      setError(message(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <SurfaceDialog
      open={connection !== null}
      onOpenChange={(open) => !open && !busy && onClose()}
      icon={<KeyRound aria-hidden />}
      title={`New token for ${shown?.name ?? "connection"}`}
      description="The old token is kept in its history, marked retired. Revoke it at the service if you haven't."
    >
      <form onSubmit={submit} className="flex flex-col gap-4">
        <Field label="Admin token" htmlFor="replace-token" error={error}>
          <Input
            id="replace-token"
            type="password"
            autoFocus
            autoComplete="off"
            className="h-9"
            value={token}
            aria-invalid={!!error}
            onChange={(e) => setToken(e.target.value)}
          />
        </Field>
        <SurfaceFooter className="pt-1">
          <Button type="button" variant="ghost" onClick={onClose} disabled={busy}>
            Cancel
          </Button>
          <Button type="submit" disabled={busy || !token.trim()}>
            {busy && <Spinner />}
            Replace token
          </Button>
        </SurfaceFooter>
      </form>
    </SurfaceDialog>
  );
}
