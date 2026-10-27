import { useEffect, useState, type FormEvent } from "react";
import { toast } from "sonner";
import { Route } from "lucide-react";
import { api, message } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Spinner } from "@/components/ui/spinner";
import { Field } from "@/components/Field";
import { SurfaceDialog } from "@/components/SurfaceDialog";
import { SurfaceFooter } from "@/components/Surface";

const EMPTY = { name: "", keyVar: "", baseUrlVar: "", upstream: "https://", basePath: "", header: "authorization" };

/** Adds a custom proxy profile. It's kept in the vault, where agents can't
 * change where a key is sent. */
export function ProxyProfileDialog({
  open,
  onOpenChange,
  onAdded,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onAdded: () => void;
}) {
  const [form, setForm] = useState(EMPTY);
  const [bearer, setBearer] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!open) return;
    setForm(EMPTY);
    setBearer(true);
    setError(null);
  }, [open]);

  const set = (key: keyof typeof EMPTY) => (e: React.ChangeEvent<HTMLInputElement>) =>
    setForm((f) => ({ ...f, [key]: e.target.value }));

  async function submit(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      await api.addProxyProfile({ ...form, bearer });
      toast.success(`Added ${form.name}`);
      onAdded();
      onOpenChange(false);
    } catch (err) {
      setError(message(err));
    } finally {
      setBusy(false);
    }
  }

  const ready = form.name.trim() && form.keyVar.trim() && form.baseUrlVar.trim() && form.upstream.trim().length > 8;
  return (
    <SurfaceDialog
      open={open}
      onOpenChange={(o) => !busy && onOpenChange(o)}
      icon={<Route aria-hidden />}
      title="Add a proxy profile"
      description="With the proxy on, the command gets a placeholder key and a local URL; dotsafe adds the real key and sends requests only to the address below."
    >
      <form onSubmit={submit} className="flex flex-col gap-4">
        <Field label="Name" htmlFor="profile-name">
          <Input id="profile-name" className="h-9" placeholder="Mistral" value={form.name} onChange={set("name")} />
        </Field>
        <div className="grid grid-cols-2 gap-3">
          <Field label="Key variable" htmlFor="profile-key">
            <Input id="profile-key" className="h-9 font-mono text-[12.5px] md:text-[12.5px]" placeholder="MISTRAL_API_KEY" value={form.keyVar} onChange={set("keyVar")} />
          </Field>
          <Field label="Base URL variable" htmlFor="profile-base" hint="The SDK reads its URL from it.">
            <Input id="profile-base" className="h-9 font-mono text-[12.5px] md:text-[12.5px]" placeholder="MISTRAL_BASE_URL" value={form.baseUrlVar} onChange={set("baseUrlVar")} />
          </Field>
        </div>
        <div className="grid grid-cols-[1fr_7rem] gap-3">
          <Field label="Upstream" htmlFor="profile-upstream" hint="The only place requests go. https:// only.">
            <Input id="profile-upstream" className="h-9" value={form.upstream} onChange={set("upstream")} />
          </Field>
          <Field label="Base path" htmlFor="profile-path">
            <Input id="profile-path" className="h-9" placeholder="/v1" value={form.basePath} onChange={set("basePath")} />
          </Field>
        </div>
        <div className="grid grid-cols-2 items-end gap-3">
          <Field label="Header" htmlFor="profile-header">
            <Input id="profile-header" className="h-9" value={form.header} onChange={set("header")} />
          </Field>
          <div className="flex h-9 items-center gap-2">
            <Checkbox id="profile-bearer" checked={bearer} onCheckedChange={(c) => setBearer(c === true)} />
            <Label htmlFor="profile-bearer">Send as Bearer token</Label>
          </div>
        </div>
        {error && (
          <p role="alert" className="text-xs text-destructive">
            {error}
          </p>
        )}
        <SurfaceFooter className="pt-1">
          <Button type="button" variant="ghost" onClick={() => onOpenChange(false)} disabled={busy}>
            Cancel
          </Button>
          <Button type="submit" disabled={busy || !ready}>
            {busy && <Spinner />}
            Add profile
          </Button>
        </SurfaceFooter>
      </form>
    </SurfaceDialog>
  );
}
