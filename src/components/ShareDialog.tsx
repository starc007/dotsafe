import { useEffect, useState, type FormEvent } from "react";
import { CircleSlash, Share2 } from "lucide-react";
import { api, message, type Usage } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { EnvironmentName } from "@/components/EnvironmentName";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Spinner } from "@/components/ui/spinner";
import { Field } from "@/components/Field";
import { SurfaceDialog } from "@/components/SurfaceDialog";
import { SurfaceFooter } from "@/components/Surface";

/** Turns a variable into a shared secret, and offers to link other
 * environments that hold the same value under the same name. Ones with the
 * same name but another value are listed too, so it's clear why they're left. */
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
  const [different, setDifferent] = useState<Usage[]>([]);
  const [chosen, setChosen] = useState<Set<string>>(new Set());
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!open) return;
    let current = true;
    setName(variable);
    setCopies(null);
    setDifferent([]);
    setError(null);
    setBusy(false);
    api
      .sharePreview(environmentId, variable)
      .then((preview) => {
        if (!current) return;
        setCopies(preview.copies);
        setDifferent(preview.different);
        setChosen(new Set(preview.copies.map((c) => c.environmentId)));
      })
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
      const link = [...chosen];
      const secret = await api.shareVariable(environmentId, variable, name, link);
      onShared(secret.name, link.length);
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
          <fieldset className="flex flex-col gap-2">
            <legend className="mb-2 flex flex-col gap-0.5">
              <span className="text-[13px] font-medium">Link it in other environments</span>
              <span className="text-xs text-muted-foreground">They have {variable} with the same value.</span>
            </legend>
            <ul className="flex flex-col gap-0.5 rounded-2xl bg-muted/50 p-1.5">
              {copies.map((c) => (
                <li key={c.environmentId}>
                  <Label className="flex cursor-pointer items-center gap-2.5 rounded-xl px-2.5 py-2 font-normal hover:bg-muted">
                    <Checkbox
                      checked={chosen.has(c.environmentId)}
                      onCheckedChange={(checked) =>
                        setChosen((prev) => {
                          const next = new Set(prev);
                          if (checked === true) next.add(c.environmentId);
                          else next.delete(c.environmentId);
                          return next;
                        })
                      }
                    />
                    <EnvironmentName usage={c} />
                  </Label>
                </li>
              ))}
            </ul>
          </fieldset>
        )}

        {different.length > 0 && (
          <div className="flex flex-col gap-1.5">
            <p className="text-xs text-muted-foreground">Not linked: {variable} has a different value in</p>
            <ul className="flex flex-col gap-1 px-1 text-[13px] text-muted-foreground">
              {different.map((c) => (
                <li key={c.environmentId} className="flex items-center gap-2">
                  <CircleSlash aria-hidden className="size-3.5 shrink-0" />
                  <EnvironmentName usage={c} muted />
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
