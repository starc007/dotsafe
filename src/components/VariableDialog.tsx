import { useState, type FormEvent, type KeyboardEvent } from "react";
import { KeyRound, Pencil } from "lucide-react";
import { api, message } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { DialogClose } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Kbd } from "@/components/ui/kbd";
import { Spinner } from "@/components/ui/spinner";
import { Textarea } from "@/components/ui/textarea";
import { Field } from "@/components/Field";
import { SurfaceFooter } from "@/components/Surface";
import { SurfaceDialog } from "@/components/SurfaceDialog";

/** Adds a variable, or edits (and renames) one when `initial` is given. */
export function VariableDialog({
  open,
  onOpenChange,
  environmentId,
  environmentLabel,
  initial,
  onSaved,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  environmentId: string;
  environmentLabel: string;
  initial?: { key: string; value: string };
  onSaved: (key: string, previousKey?: string) => void;
}) {
  const editing = !!initial;
  return (
    <SurfaceDialog
      open={open}
      onOpenChange={onOpenChange}
      icon={editing ? <Pencil aria-hidden /> : <KeyRound aria-hidden />}
      title={editing ? `Edit ${initial.key}` : "Add variable"}
      description={`In ${environmentLabel}`}
    >
      {/* Keyed so each opening starts from fresh state. */}
      <VariableForm key={initial?.key ?? "new"} environmentId={environmentId} initial={initial} onSaved={onSaved} />
    </SurfaceDialog>
  );
}

function VariableForm({
  environmentId,
  initial,
  onSaved,
}: {
  environmentId: string;
  initial?: { key: string; value: string };
  onSaved: (key: string, previousKey?: string) => void;
}) {
  const [key, setKey] = useState(initial?.key ?? "");
  const [value, setValue] = useState(initial?.value ?? "");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function submit(e?: FormEvent) {
    e?.preventDefault();
    if (!key.trim()) return;
    setBusy(true);
    setError(null);
    try {
      await api.setVariable(environmentId, key, value, initial?.key);
      onSaved(key.trim(), initial?.key);
    } catch (err) {
      setError(message(err));
      setBusy(false);
    }
  }

  // Values can span lines (certificates, keys), so ⌘↵ saves from the value box.
  function onValueKeyDown(e: KeyboardEvent<HTMLTextAreaElement>) {
    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      submit();
    }
  }

  return (
    <form onSubmit={submit} className="flex flex-col gap-4">
      <Field label="Name" htmlFor="var-name" error={error}>
        <Input
          id="var-name"
          placeholder="API_KEY"
          autoFocus={!initial}
          autoComplete="off"
          autoCorrect="off"
          autoCapitalize="off"
          spellCheck={false}
          value={key}
          aria-invalid={!!error}
          onChange={(e) => setKey(e.target.value.replace(/\s/g, "_"))}
          className="h-9 font-mono text-[12.5px] md:text-[12.5px]"
        />
      </Field>
      <Field
        label="Value"
        htmlFor="var-value"
        hint={
          <span className="flex items-center gap-1">
            Encrypted before it's saved. <Kbd>⌘</Kbd>
            <Kbd>↵</Kbd> to save.
          </span>
        }
      >
        <Textarea
          id="var-value"
          placeholder="Paste or type the value"
          autoFocus={!!initial}
          autoComplete="off"
          autoCorrect="off"
          autoCapitalize="off"
          spellCheck={false}
          value={value}
          onKeyDown={onValueKeyDown}
          onChange={(e) => setValue(e.target.value)}
          className="max-h-60 min-h-24 resize-none font-mono text-[12.5px] md:text-[12.5px]"
        />
      </Field>
      <SurfaceFooter className="pt-1">
        <DialogClose asChild>
          <Button type="button" variant="ghost">
            Cancel
          </Button>
        </DialogClose>
        <Button type="submit" disabled={busy || !key.trim()}>
          {busy && <Spinner />}
          {initial ? "Save" : "Add variable"}
        </Button>
      </SurfaceFooter>
    </form>
  );
}
