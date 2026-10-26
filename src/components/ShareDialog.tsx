import { useEffect, useState, type FormEvent } from "react";
import { Share2 } from "lucide-react";
import { api, message, type Usage } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Spinner } from "@/components/ui/spinner";
import { Field } from "@/components/Field";
import { SurfaceDialog } from "@/components/SurfaceDialog";
import { SurfaceFooter } from "@/components/Surface";

/** Turns a variable into a shared secret, and offers to link other
 * environments that hold the same value under the same name. */
export function ShareDialog({
  open,
  onOpenChange,
  environmentId,
  variable,
  onShared,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  environmentId: string;
  variable: string;
  onShared: (name: string, linked: number) => void;
}) {
  const [name, setName] = useState(variable);
  const [copies, setCopies] = useState<Usage[] | null>(null);
  const [linkCopies, setLinkCopies] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!open) return;
    let current = true;
    setName(variable);
    setCopies(null);
    setLinkCopies(true);
    setError(null);
    setBusy(false);
    api
      .sharePreview(environmentId, variable)
      .then((preview) => current && setCopies(preview.copies))
      .catch((e) => current && setError(message(e)));
    return () => {
      current = false;
    };
  }, [open, environmentId, variable]);

  async function submit(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      const link = linkCopies && !!copies?.length;
      const secret = await api.shareVariable(environmentId, variable, name, link);
      onShared(secret.name, link ? copies!.length : 0);
    } catch (err) {
      setError(message(err));
      setBusy(false);
    }
  }

  return (
    <SurfaceDialog
      open={open}
      onOpenChange={onOpenChange}
      icon={<Share2 aria-hidden />}
      title={`Share ${variable}`}
      description="Define it once, link it anywhere, and rotate it in one place."
    >
      <form onSubmit={submit} className="flex flex-col gap-4">
        <Field label="Secret name" htmlFor="secret-name" error={error} hint="Shown in Shared secrets. Variable names stay as they are.">
          <Input
            id="secret-name"
            autoFocus
            className="h-9 font-mono text-[12.5px] md:text-[12.5px]"
            autoComplete="off"
            spellCheck={false}
            value={name}
            aria-invalid={!!error}
            onChange={(e) => setName(e.target.value)}
          />
        </Field>

        {copies && copies.length > 0 && (
          <div className="flex flex-col gap-2 rounded-2xl bg-muted/50 p-3">
            <div className="flex items-start gap-2.5">
              <Checkbox
                id="link-copies"
                checked={linkCopies}
                onCheckedChange={(checked) => setLinkCopies(checked === true)}
                className="mt-0.5"
              />
              <Label htmlFor="link-copies" className="leading-snug font-normal">
                Also link {copies.length === 1 ? "1 other environment" : `${copies.length} other environments`} with
                the same value
              </Label>
            </div>
            <ul className="ml-6.5 flex flex-col gap-0.5 text-xs text-muted-foreground">
              {copies.map((c) => (
                <li key={c.environmentId} className="truncate">
                  {c.project}/{c.environment}
                </li>
              ))}
            </ul>
          </div>
        )}

        <SurfaceFooter className="pt-1">
          <Button type="button" variant="ghost" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button type="submit" disabled={busy || copies === null || !name.trim()}>
            {busy && <Spinner />}
            Share
          </Button>
        </SurfaceFooter>
      </form>
    </SurfaceDialog>
  );
}
