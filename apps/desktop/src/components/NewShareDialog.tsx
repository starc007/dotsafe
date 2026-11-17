import { useEffect, useState, type FormEvent } from "react";
import { toast } from "sonner";
import { Copy, Server } from "lucide-react";
import { api, message, type CreatedShare, type ProjectView } from "@/lib/api";
import { formatDate } from "@/lib/time";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { Spinner } from "@/components/ui/spinner";
import { EnvironmentName } from "@/components/EnvironmentName";
import { Field } from "@/components/Field";
import { SurfaceDialog } from "@/components/SurfaceDialog";
import { SurfaceFooter } from "@/components/Surface";

const LASTS = [1, 7, 30, 90, 365];
const LEASES = [7, 14, 30];

function days(n: number) {
  if (n === 365) return "1 year";
  return n === 1 ? "1 day" : `${n} days`;
}

/** Creates a standing share, then shows its token once. The token stays in
 * Rust: the dialog shows its start and end, and copying happens in Rust. */
export function NewShareDialog({
  open,
  onOpenChange,
  onCreated,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onCreated: () => void;
}) {
  const [projects, setProjects] = useState<ProjectView[] | null>(null);
  const [name, setName] = useState("");
  const [chosen, setChosen] = useState<Set<string>>(new Set());
  const [only, setOnly] = useState("");
  const [lasts, setLasts] = useState(30);
  const [lease, setLease] = useState(14);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [created, setCreated] = useState<CreatedShare | null>(null);

  useEffect(() => {
    if (!open) return;
    let current = true;
    setProjects(null);
    setName("");
    setChosen(new Set());
    setOnly("");
    setLasts(30);
    setLease(14);
    setError(null);
    setBusy(false);
    setCreated(null);
    api.listProjects().then(
      (list) => current && setProjects(list),
      (e) => current && setError(message(e)),
    );
    return () => {
      current = false;
    };
  }, [open]);

  function close(next: boolean) {
    if (busy || next) return;
    // The token is only kept while this is open.
    if (created) api.forgetShareToken().catch(() => {});
    onOpenChange(false);
  }

  async function submit(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      const names = only.split(",").map((n) => n.trim()).filter(Boolean);
      setCreated(
        await api.createShare({
          name,
          environmentIds: [...chosen],
          only: names.length ? names : null,
          days: lasts,
          leaseDays: Math.min(lease, lasts),
        }),
      );
      onCreated();
    } catch (err) {
      setError(message(err));
    } finally {
      setBusy(false);
    }
  }

  async function copy() {
    try {
      await api.copyShareToken();
      toast("Copied the token", { description: "The clipboard clears in 30 seconds." });
    } catch (e) {
      toast.error(message(e));
    }
  }

  const environments = projects?.flatMap((p) => p.environments.map((e) => ({ id: e.id, project: p.name, environment: e.name }))) ?? [];

  return (
    <SurfaceDialog
      open={open}
      onOpenChange={close}
      icon={<Server aria-hidden />}
      title={created ? `Give ${created.name} this token` : "New share"}
      description={
        created
          ? "It's shown once and never saved. If it's lost, revoke the share and make a new one."
          : "A machine gets the final values of the environments you pick, and nothing else."
      }
    >
      {created ? (
          <div className="flex flex-col gap-4">
            <div className="flex items-center gap-3 rounded-2xl bg-muted/60 p-3">
              <code aria-label="Token, shortened" className="min-w-0 flex-1 truncate font-mono text-[12.5px]">
                {created.preview}
              </code>
              <Button onClick={copy}>
                <Copy aria-hidden />
                Copy token
              </Button>
            </div>
            <div className="flex flex-col gap-1.5">
              <p className="text-xs text-muted-foreground">
                On the machine, set it as <code className="font-mono">DOTSAFE_SHARE</code> and run:
              </p>
              <code className="selectable rounded-xl bg-muted/60 px-3 py-2 font-mono text-[12px]">
                dotsafe run --share -- &lt;command&gt;
              </code>
              <p className="text-xs text-muted-foreground">
                It lasts until {formatDate(created.expires)}. Edits here reach it within seconds while dotsafe is unlocked.
              </p>
            </div>
            <SurfaceFooter className="pt-1">
              <Button onClick={() => close(false)}>Done</Button>
            </SurfaceFooter>
          </div>
        ) : (
          <form onSubmit={submit} className="flex flex-col gap-4">
            <Field label="Name" htmlFor="share-name" hint="What it's for, like ci or staging-vm.">
              <Input
                id="share-name"
                autoFocus
                className="h-9"
                autoComplete="off"
                spellCheck={false}
                value={name}
                onChange={(e) => setName(e.target.value)}
              />
            </Field>

            <fieldset className="flex flex-col gap-1.5">
              <legend className="mb-1.5 text-xs text-muted-foreground">Environments</legend>
              {projects === null ? (
                <Skeleton aria-hidden className="h-24 w-full rounded-2xl" />
              ) : environments.length === 0 ? (
                <p className="text-muted-foreground">No environments yet.</p>
              ) : (
                <ul className="flex max-h-48 flex-col gap-0.5 overflow-y-auto rounded-2xl bg-muted/50 p-1.5">
                  {environments.map((env) => (
                    <li key={env.id}>
                      <Label className="flex cursor-pointer items-center gap-2.5 rounded-xl px-2.5 py-1.5 font-normal hover:bg-muted">
                        <Checkbox
                          checked={chosen.has(env.id)}
                          onCheckedChange={(checked) =>
                            setChosen((prev) => {
                              const next = new Set(prev);
                              if (checked === true) next.add(env.id);
                              else next.delete(env.id);
                              return next;
                            })
                          }
                        />
                        <EnvironmentName usage={env} />
                      </Label>
                    </li>
                  ))}
                </ul>
              )}
            </fieldset>

            <Field label="Only these variables" htmlFor="share-only" hint="Optional, comma-separated. Leave it empty for every variable.">
              <Input
                id="share-only"
                className="h-9 font-mono text-[12.5px] md:text-[12.5px]"
                autoComplete="off"
                spellCheck={false}
                placeholder="DATABASE_URL, OPENAI_API_KEY"
                value={only}
                onChange={(e) => setOnly(e.target.value)}
              />
            </Field>

            <div className="grid grid-cols-2 gap-3">
              <Field label="Lasts" htmlFor="share-lasts">
                <Select value={String(lasts)} onValueChange={(v) => setLasts(Number(v))}>
                  <SelectTrigger id="share-lasts" className="h-9 w-full">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {LASTS.map((n) => (
                      <SelectItem key={n} value={String(n)}>
                        {days(n)}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </Field>
              <Field label="Works without your devices for" htmlFor="share-lease">
                <Select value={String(lease)} onValueChange={(v) => setLease(Number(v))}>
                  <SelectTrigger id="share-lease" className="h-9 w-full">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {LEASES.map((n) => (
                      <SelectItem key={n} value={String(n)}>
                        {days(n)}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </Field>
            </div>

            {error && (
              <p role="alert" className="selectable text-xs text-destructive">
                {error}
              </p>
            )}
            <SurfaceFooter className="pt-1">
              <Button type="button" variant="ghost" onClick={() => close(false)} disabled={busy}>
                Cancel
              </Button>
              <Button type="submit" disabled={busy || !name.trim() || chosen.size === 0}>
                {busy && <Spinner />}
                Create share
              </Button>
            </SurfaceFooter>
          </form>
        )}
    </SurfaceDialog>
  );
}
