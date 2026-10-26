import { useEffect, useMemo, useState } from "react";
import { AnimatePresence, motion } from "motion/react";
import { FileText, FileUp, Upload } from "lucide-react";
import { api, message, type ImportPreview, type ImportResult, type ImportSource } from "@/lib/api";
import { fast } from "@/lib/motion";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { DialogClose } from "@/components/ui/dialog";
import { Kbd } from "@/components/ui/kbd";
import { Label } from "@/components/ui/label";
import { Spinner } from "@/components/ui/spinner";
import { Tabs, TabsList, TabsTrigger } from "@/components/motion/tabs";
import { Textarea } from "@/components/ui/textarea";
import { SurfaceFooter } from "@/components/Surface";
import { SurfaceDialog } from "@/components/SurfaceDialog";

type Mode = "files" | "paste";

const CHANGE_LABEL = { new: "New", update: "Changes", unchanged: "Same" } as const;

/** Imports many variables at once from .env files (picked or dropped) or pasted text. */
export function ImportDialog({
  open,
  onOpenChange,
  environmentId,
  environmentLabel,
  droppedFiles,
  onImported,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  environmentId: string;
  environmentLabel: string;
  /** Names of files dropped on the window; Rust already holds their paths. */
  droppedFiles?: string[];
  onImported: (result: ImportResult) => void;
}) {
  function close(next: boolean) {
    // Forget picked files on cancel so a later import can't pick them up by accident.
    if (!next) api.clearImportFiles().catch(() => {});
    onOpenChange(next);
  }

  return (
    <SurfaceDialog
      open={open}
      onOpenChange={close}
      icon={<Upload aria-hidden />}
      title="Import variables"
      description={`Into ${environmentLabel}`}
      className="sm:max-w-lg"
    >
      <ImportForm
        key={droppedFiles?.join("\n") ?? "fresh"}
        environmentId={environmentId}
        droppedFiles={droppedFiles}
        onImported={onImported}
      />
    </SurfaceDialog>
  );
}

/** The import flow without its dialog, so the add-variable dialog can reuse
 * it when several variables are pasted at once. */
export function ImportForm({
  environmentId,
  droppedFiles,
  initialText,
  pasteOnly,
  verb = "Import",
  onImported,
}: {
  environmentId: string;
  droppedFiles?: string[];
  /** Start in paste mode with this text. */
  initialText?: string;
  /** Show just the paste box, without the files/paste tabs. */
  pasteOnly?: boolean;
  verb?: string;
  onImported: (result: ImportResult) => void;
}) {
  const [mode, setMode] = useState<Mode>(pasteOnly || initialText ? "paste" : "files");
  const [files, setFiles] = useState<string[]>(droppedFiles ?? []);
  const [text, setText] = useState(initialText ?? "");
  const [preview, setPreview] = useState<ImportPreview | null>(null);
  const [previewing, setPreviewing] = useState(false);
  const [overwrite, setOverwrite] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const source: ImportSource | null = useMemo(() => {
    if (mode === "files") return files.length ? { kind: "files" } : null;
    return text.trim() ? { kind: "text", text } : null;
  }, [mode, files, text]);

  // Preview as the input changes; debounced so typing in the paste box stays smooth.
  useEffect(() => {
    if (!source) {
      setPreview(null);
      return;
    }
    let current = true;
    setPreviewing(true);
    const timer = window.setTimeout(async () => {
      try {
        const next = await api.previewImport(environmentId, source);
        if (current) setPreview(next);
      } catch (e) {
        if (current) setError(message(e));
      } finally {
        if (current) setPreviewing(false);
      }
    }, mode === "paste" ? 250 : 0);
    return () => {
      current = false;
      window.clearTimeout(timer);
    };
  }, [environmentId, source, mode]);

  async function chooseFiles() {
    setError(null);
    try {
      const picked = await api.pickEnvFiles();
      if (picked.length) setFiles(picked);
    } catch (e) {
      setError(message(e));
    }
  }

  async function runImport() {
    if (!source) return;
    setBusy(true);
    setError(null);
    try {
      onImported(await api.importVariables(environmentId, source, overwrite));
    } catch (e) {
      setError(message(e));
      setBusy(false);
    }
  }

  const counts = { new: 0, update: 0, unchanged: 0 };
  preview?.entries.forEach((e) => counts[e.change]++);
  const toImport = counts.new + (overwrite ? counts.update : 0);
  const showFileNames = mode === "files" && files.length > 1;

  const pasteBox = (
    <Textarea
      aria-label=".env contents"
      autoFocus
      autoComplete="off"
      autoCorrect="off"
      autoCapitalize="off"
      spellCheck={false}
      placeholder={"DATABASE_URL=postgres://localhost/app\nSTRIPE_SECRET_KEY=sk_test_…\n# comments and export are fine"}
      value={text}
      onChange={(e) => setText(e.target.value)}
      className="max-h-56 min-h-32 resize-none font-mono text-[12.5px] md:text-[12.5px]"
    />
  );

  return (
    <div className="flex flex-col gap-4">
      {pasteOnly ? (
        pasteBox
      ) : (
        <>
          <Tabs value={mode} onValueChange={(v) => setMode(v as Mode)}>
            <TabsList aria-label="Import from" className="bg-muted p-0.5">
              <TabsTrigger value="files" icon={<FileUp className="size-3.5" />} className="px-3 py-1 text-[13px]">
                From files
              </TabsTrigger>
              <TabsTrigger value="paste" icon={<FileText className="size-3.5" />} className="px-3 py-1 text-[13px]">
                Paste text
              </TabsTrigger>
            </TabsList>
          </Tabs>
          {mode === "paste" ? (
            pasteBox
          ) : files.length === 0 ? (
            <div className="flex flex-col items-center gap-3 rounded-2xl bg-muted/50 px-6 py-8 text-center">
              <div className="flex size-10 items-center justify-center rounded-xl bg-card text-muted-foreground">
                <FileUp aria-hidden className="size-5" />
              </div>
              <div>
                <p className="font-medium">Drop .env files anywhere in the window</p>
                <p className="mt-0.5 text-muted-foreground">or choose one or more from disk</p>
              </div>
              <Button variant="outline" onClick={chooseFiles} className="bg-card">
                Choose files…
              </Button>
              <p className="flex items-center gap-1 text-xs text-muted-foreground">
                Can't see .env? Press <Kbd>⌘</Kbd>
                <Kbd>⇧</Kbd>
                <Kbd>.</Kbd> in the picker to show hidden files.
              </p>
            </div>
          ) : (
            <div className="flex items-center gap-2 rounded-2xl bg-muted/50 p-2 pl-3">
              <FileText aria-hidden className="size-4 shrink-0 text-muted-foreground" />
              <p className="min-w-0 flex-1 truncate font-mono text-[12.5px]" title={files.join(", ")}>
                {files.join(", ")}
              </p>
              <Button size="sm" variant="ghost" onClick={chooseFiles}>
                Change…
              </Button>
            </div>
          )}
        </>
      )}

      <AnimatePresence initial={false}>
        {preview && (preview.entries.length > 0 || preview.errors.length > 0) && (
          <motion.div
            key="preview"
            initial={{ opacity: 0, height: 0 }}
            animate={{ opacity: 1, height: "auto" }}
            exit={{ opacity: 0, height: 0 }}
            transition={fast}
            className="overflow-hidden"
          >
            <div className="flex flex-col gap-2">
              {preview.entries.length > 0 && (
                <>
                  <p className="flex items-center gap-2 text-xs text-muted-foreground">
                    <span>
                      {[
                        counts.new && `${counts.new} new`,
                        counts.update && `${counts.update} with a different value`,
                        counts.unchanged && `${counts.unchanged} already the same`,
                      ]
                        .filter(Boolean)
                        .join(" · ")}
                    </span>
                    {previewing && <Spinner className="size-3" />}
                  </p>
                  <ul aria-label="Variables to import" className="max-h-52 divide-y overflow-y-auto rounded-2xl bg-muted/40">
                    {preview.entries.map((e) => (
                      <li key={e.key} className="flex items-center gap-3 px-3 py-2">
                        <span className="min-w-0 flex-1 truncate font-mono text-[12.5px]">{e.key}</span>
                        {showFileNames && e.file && (
                          <span className="truncate text-xs text-muted-foreground">{e.file}</span>
                        )}
                        <span
                          className={cn(
                            "shrink-0 rounded-full px-2 py-0.5 text-[11px] font-medium",
                            e.change === "new" && "bg-primary/10 text-primary",
                            e.change === "update" && "bg-foreground/10 text-foreground",
                            e.change === "unchanged" && "text-muted-foreground",
                          )}
                        >
                          {CHANGE_LABEL[e.change]}
                        </span>
                      </li>
                    ))}
                  </ul>
                </>
              )}
              {preview.errors.map((err) => (
                <p key={err} role="alert" className="text-xs text-destructive">
                  {err}
                </p>
              ))}
            </div>
          </motion.div>
        )}
      </AnimatePresence>

      {counts.update > 0 && (
        <div className="flex items-start gap-2">
          <Checkbox id="overwrite" checked={overwrite} onCheckedChange={(v) => setOverwrite(v === true)} className="mt-px" />
          <Label htmlFor="overwrite" className="font-normal leading-snug">
            Replace {counts.update === 1 ? "the existing value" : `${counts.update} existing values`} with the imported
            {counts.update === 1 ? " one" : " ones"}
          </Label>
        </div>
      )}

      {error && (
        <p role="alert" className="text-xs text-destructive">
          {error}
        </p>
      )}

      <SurfaceFooter className="pt-1">
        <DialogClose asChild>
          <Button variant="ghost">Cancel</Button>
        </DialogClose>
        <Button onClick={runImport} disabled={busy || toImport === 0}>
          {busy && <Spinner />}
          {toImport === 0 ? verb : `${verb} ${toImport} ${toImport === 1 ? "variable" : "variables"}`}
        </Button>
      </SurfaceFooter>
    </div>
  );
}
