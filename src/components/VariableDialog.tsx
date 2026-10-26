import { useEffect, useState, type ClipboardEvent, type FormEvent, type KeyboardEvent } from "react";
import { KeyRound, ListPlus, Pencil } from "lucide-react";
import { api, message, type ImportResult } from "@/lib/api";
import { countAssignments, splitAssignment } from "@/lib/envText";
import { Button } from "@/components/ui/button";
import { DialogClose } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Kbd } from "@/components/ui/kbd";
import { Spinner } from "@/components/ui/spinner";
import { Textarea } from "@/components/ui/textarea";
import { Field } from "@/components/Field";
import { ImportForm } from "@/components/ImportDialog";
import { SurfaceFooter } from "@/components/Surface";
import { SurfaceDialog } from "@/components/SurfaceDialog";

/** Adds a variable, or edits (and renames) one when `initial` is given.
 * Pasting several `KEY=value` lines switches to adding them all at once. */
export function VariableDialog({
  open,
  onOpenChange,
  environmentId,
  environmentLabel,
  initial,
  onSaved,
  onImported,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  environmentId: string;
  environmentLabel: string;
  initial?: { key: string; value: string };
  onSaved: (key: string, previousKey?: string) => void;
  onImported: (result: ImportResult) => void;
}) {
  const editing = !!initial;
  const [bulkText, setBulkText] = useState<string | null>(null);

  // Each opening starts as a single-variable form.
  useEffect(() => {
    if (open) setBulkText(null);
  }, [open]);

  if (bulkText !== null) {
    return (
      <SurfaceDialog
        open={open}
        onOpenChange={onOpenChange}
        icon={<ListPlus aria-hidden />}
        title="Add variables"
        description={`Pasted into ${environmentLabel}`}
        className="sm:max-w-lg"
      >
        <div className="flex flex-col gap-3">
          <p className="text-xs text-muted-foreground">
            That looked like several variables, so they'll be added together.{" "}
            <button
              type="button"
              onClick={() => setBulkText(null)}
              className="font-medium text-foreground underline-offset-2 hover:underline focus-visible:underline focus-visible:outline-none"
            >
              Add one at a time instead
            </button>
          </p>
          <ImportForm environmentId={environmentId} initialText={bulkText} pasteOnly verb="Add" onImported={onImported} />
        </div>
      </SurfaceDialog>
    );
  }

  return (
    <SurfaceDialog
      open={open}
      onOpenChange={onOpenChange}
      icon={editing ? <Pencil aria-hidden /> : <KeyRound aria-hidden />}
      title={editing ? `Edit ${initial.key}` : "Add variable"}
      description={`In ${environmentLabel}`}
    >
      {/* Keyed so each opening starts from fresh state. */}
      <VariableForm
        key={initial?.key ?? "new"}
        environmentId={environmentId}
        initial={initial}
        onSaved={onSaved}
        onBulkPaste={setBulkText}
      />
    </SurfaceDialog>
  );
}

function VariableForm({
  environmentId,
  initial,
  onSaved,
  onBulkPaste,
}: {
  environmentId: string;
  initial?: { key: string; value: string };
  onSaved: (key: string, previousKey?: string) => void;
  onBulkPaste: (text: string) => void;
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

  // A pasted .env snippet is handled for you: several lines switch to adding
  // them all, and a single KEY=value pasted into Name fills both fields.
  function onPaste(e: ClipboardEvent<HTMLInputElement | HTMLTextAreaElement>, field: "name" | "value") {
    const text = e.clipboardData.getData("text");
    if (!initial && countAssignments(text) >= 2 && (field === "name" || !key.trim())) {
      e.preventDefault();
      onBulkPaste(text);
      return;
    }
    if (field === "name") {
      const pair = splitAssignment(text);
      if (pair) {
        e.preventDefault();
        setKey(pair.key);
        if (pair.value) setValue(pair.value);
      }
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
          placeholder="API_KEY, or paste KEY=value lines"
          autoFocus={!initial}
          autoComplete="off"
          autoCorrect="off"
          autoCapitalize="off"
          spellCheck={false}
          value={key}
          aria-invalid={!!error}
          onPaste={(e) => onPaste(e, "name")}
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
          onPaste={(e) => onPaste(e, "value")}
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
