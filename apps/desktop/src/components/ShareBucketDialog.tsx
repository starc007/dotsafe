import { useEffect, useState, type FormEvent } from "react";
import { toast } from "sonner";
import { Database } from "lucide-react";
import { api, message, type ShareBucketView } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Spinner } from "@/components/ui/spinner";
import { Field } from "@/components/Field";
import { SurfaceDialog } from "@/components/SurfaceDialog";
import { SurfaceFooter } from "@/components/Surface";

const EMPTY = { endpoint: "", region: "auto", bucket: "", publicUrl: "", accessKeyId: "", secretAccessKey: "" };

/** Sets up the share bucket on this Mac. dotsafe checks it's public, unlisted
 * and takes expiry rules before saving; the key goes to the keychain. */
export function ShareBucketDialog({
  open,
  current,
  onOpenChange,
  onSaved,
}: {
  open: boolean;
  current: ShareBucketView | null;
  onOpenChange: (open: boolean) => void;
  onSaved: () => void;
}) {
  const [form, setForm] = useState(EMPTY);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!open) return;
    setForm(current ? { ...EMPTY, ...current } : EMPTY);
    setError(null);
    setBusy(false);
  }, [open, current]);

  const set = (key: keyof typeof EMPTY) => (e: React.ChangeEvent<HTMLInputElement>) =>
    setForm((f) => ({ ...f, [key]: e.target.value }));

  async function submit(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      const warnings = await api.setUpShareBucket(form);
      toast.success(`Shares go to ${form.bucket.trim()}`);
      for (const w of warnings) toast(w);
      onSaved();
      onOpenChange(false);
    } catch (err) {
      setError(message(err));
    } finally {
      setBusy(false);
    }
  }

  const ready = form.endpoint.trim() && form.bucket.trim() && form.publicUrl.trim() && form.accessKeyId.trim() && form.secretAccessKey;

  return (
    <SurfaceDialog
      open={open}
      onOpenChange={(next) => !busy && onOpenChange(next)}
      icon={<Database aria-hidden />}
      title="Share bucket"
      description="Use a bucket only for shares, not the vault's. Make it public-read with listing off."
    >
      <form onSubmit={submit} className="flex flex-col gap-4">
        <Field label="S3 endpoint" htmlFor="share-endpoint" hint="Where this Mac writes, e.g. https://<account>.r2.cloudflarestorage.com">
          <Input id="share-endpoint" autoFocus className="h-9" spellCheck={false} autoComplete="off" value={form.endpoint} onChange={set("endpoint")} />
        </Field>
        <div className="grid grid-cols-[1fr_8rem] gap-3">
          <Field label="Bucket" htmlFor="share-bucket">
            <Input id="share-bucket" className="h-9" spellCheck={false} autoComplete="off" value={form.bucket} onChange={set("bucket")} />
          </Field>
          <Field label="Region" htmlFor="share-region">
            <Input id="share-region" className="h-9" spellCheck={false} autoComplete="off" value={form.region} onChange={set("region")} />
          </Field>
        </div>
        <Field label="Public URL" htmlFor="share-public-url" hint="Where machines read, https only: an R2 custom domain or the bucket's public address.">
          <Input id="share-public-url" className="h-9" spellCheck={false} autoComplete="off" value={form.publicUrl} onChange={set("publicUrl")} />
        </Field>
        <div className="grid grid-cols-2 gap-3">
          <Field label="Access key ID" htmlFor="share-key-id">
            <Input id="share-key-id" className="h-9" spellCheck={false} autoComplete="off" value={form.accessKeyId} onChange={set("accessKeyId")} />
          </Field>
          <Field label="Secret access key" htmlFor="share-key-secret">
            <Input id="share-key-secret" type="password" className="h-9" autoComplete="off" value={form.secretAccessKey} onChange={set("secretAccessKey")} />
          </Field>
        </div>
        {error && (
          <p role="alert" className="selectable text-xs text-destructive">
            {error}
          </p>
        )}
        <SurfaceFooter className="pt-1">
          <Button type="button" variant="ghost" onClick={() => onOpenChange(false)} disabled={busy}>
            Cancel
          </Button>
          <Button type="submit" disabled={busy || !ready}>
            {busy && <Spinner />}
            Check and save
          </Button>
        </SurfaceFooter>
      </form>
    </SurfaceDialog>
  );
}
