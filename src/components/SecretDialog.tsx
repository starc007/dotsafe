import { useCallback, useEffect, useRef, useState, type FormEvent } from "react";
import { toast } from "sonner";
import { Check, Copy, ExternalLink, Eye, EyeOff, History, KeyRound, RotateCcw, ShieldAlert, Sparkles } from "lucide-react";
import { api, message, type SecretDetail, type SecretIntegrations, type VersionState } from "@/lib/api";
import { daysAgo, formatDate } from "@/lib/time";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import { Spinner } from "@/components/ui/spinner";
import { Tabs, TabsList, TabsTrigger } from "@/components/motion/tabs";
import { ActionSwapIcon, ActionSwapText } from "@/components/motion/action-swap";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { Field } from "@/components/Field";
import { IconButton } from "@/components/IconButton";
import { SecretIntegrationsSection, reportPushes } from "@/components/SecretIntegrationsSection";
import { SurfaceDialog } from "@/components/SurfaceDialog";
import { SurfaceFooter } from "@/components/Surface";

const MASK = "••••••••••••";
const REVEAL_MS = 30_000;
const POLICIES = [
  { value: "off", label: "Off", days: null },
  { value: "30", label: "30 days", days: 30 },
  { value: "90", label: "90 days", days: 90 },
  { value: "180", label: "180 days", days: 180 },
  { value: "365", label: "1 year", days: 365 },
] as const;

const STATE_LABEL: Record<VersionState, string> = {
  pending: "Staged",
  current: "Current",
  previous: "Previous",
  retired: "Retired",
};

/** `guided`: pasting a key made in a guided provider's console. */
type Mode = "idle" | "rotate" | "leaked" | "guided";

/** One shared secret: its value, the rotation flow, where it's used, its
 * history and its settings. */
export function SecretDialog({
  secretId,
  open,
  onOpenChange,
  onChanged,
  onOpenEnvironment,
}: {
  secretId: string | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onChanged: () => void;
  onOpenEnvironment: (projectId: string, environmentId: string) => void;
}) {
  const [secret, setSecret] = useState<SecretDetail | null>(null);
  const [integrations, setIntegrations] = useState<SecretIntegrations | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [revealed, setRevealed] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [mode, setMode] = useState<Mode>("idle");
  const [busy, setBusy] = useState<string | null>(null);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const hideTimer = useRef<number | undefined>(undefined);

  const load = useCallback(async () => {
    if (!secretId) return;
    try {
      const [detail, links] = await Promise.all([api.getSecret(secretId), api.secretIntegrations(secretId)]);
      setSecret(detail);
      setIntegrations(links);
      setError(null);
    } catch (e) {
      setError(message(e));
    }
  }, [secretId]);

  useEffect(() => {
    if (!open) return;
    setSecret(null);
    setIntegrations(null);
    setRevealed(null);
    setMode("idle");
    load();
    return () => window.clearTimeout(hideTimer.current);
  }, [open, load]);

  /** Runs a change, shows the updated secret, and tells the page to refresh. */
  async function act(label: string, run: () => Promise<SecretDetail | void>, success?: string) {
    setBusy(label);
    try {
      const updated = await run();
      if (updated) setSecret(updated);
      setRevealed(null);
      if (success) toast.success(success);
      onChanged();
      load();
      return true;
    } catch (e) {
      toast.error(message(e));
      return false;
    } finally {
      setBusy(null);
    }
  }

  async function toggleReveal() {
    if (!secret) return;
    if (revealed !== null) return setRevealed(null);
    try {
      setRevealed(await api.revealSecret(secret.id));
      window.clearTimeout(hideTimer.current);
      hideTimer.current = window.setTimeout(() => setRevealed(null), REVEAL_MS);
    } catch (e) {
      toast.error(message(e));
    }
  }

  async function copy() {
    if (!secret) return;
    try {
      await api.copySecret(secret.id);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1500);
      toast(`Copied ${secret.name}`, { description: "The clipboard clears in 30 seconds." });
    } catch (e) {
      toast.error(message(e));
    }
  }

  const uses = secret?.usedIn.length ?? 0;
  const provider = integrations?.provider ?? null;

  /** After a value goes live by hand, push it to the targets too. */
  async function pushIfTargets() {
    if (!secret || !integrations?.targets.length) return;
    try {
      reportPushes(await api.pushSecret(secret.id));
      load();
    } catch (e) {
      toast.error(message(e));
    }
  }

  async function rotateWithProvider(leaked: boolean, pasted: string | null = null) {
    if (!secret || !provider) return;
    setBusy(leaked ? "leaked" : "provider");
    try {
      const out = await api.rotateWithProvider(secret.id, leaked, pasted);
      if (!out.promoted) {
        toast.error(`The new key ${provider.guided ? "" : `from ${provider.connectionName} `}didn't pass its check`, {
          description: `${out.checkFailed} It's saved as staged; nothing uses it yet.`,
        });
      } else {
        toast.success(
          out.revokedOld
            ? `${secret.name} has a new key and the old one is revoked`
            : provider.guided
              ? `${secret.name} uses the new key`
              : `${secret.name} has a new key from ${provider.connectionName}`,
          out.checkFailed ? { description: `The check failed (${out.checkFailed}), but the old key had already stopped working.` } : undefined,
        );
      }
      reportPushes(out.pushes);
      if (leaked && provider.manualRevoke)
        toast(`Revoke the leaked key in ${provider.serviceLabel} now`, { description: "Then mark it retired here." });
      setMode("idle");
      onChanged();
      load();
    } catch (e) {
      toast.error(message(e));
    } finally {
      setBusy(null);
    }
  }

  return (
    <>
      <SurfaceDialog
        open={open}
        onOpenChange={onOpenChange}
        icon={<KeyRound aria-hidden />}
        title={secret?.name ?? "Shared secret"}
        description={
          secret
            ? `Rotated ${secret.rotatedAt ? daysAgo(secret.rotatedAt) : "never"} · used in ${uses} ${uses === 1 ? "place" : "places"}`
            : " "
        }
        className="sm:max-w-lg"
      >
        {error ? (
          <p className="text-destructive">{error}</p>
        ) : !secret ? (
          <div className="flex flex-col gap-3">
            <Skeleton className="h-9 w-full rounded-full" />
            <Skeleton className="h-16 w-full rounded-2xl" />
            <Skeleton className="h-24 w-full rounded-2xl" />
          </div>
        ) : (
          <div className="-mx-5 -my-5 flex max-h-[min(70vh,640px)] flex-col gap-5 overflow-y-auto p-5">
            <section aria-label="Current value">
              <div className="flex items-center gap-1 rounded-full bg-muted/60 py-1 pr-1 pl-3.5">
                <span className="min-w-0 flex-1 truncate font-mono text-[12.5px]">
                  <ActionSwapText value={revealed !== null ? "value" : "mask"} animation="roll">
                    <span className={revealed !== null ? "selectable" : "tracking-widest text-muted-foreground"}>
                      {revealed ?? MASK}
                    </span>
                  </ActionSwapText>
                </span>
                <IconButton label={revealed !== null ? "Hide value" : "Reveal value"} onClick={toggleReveal}>
                  <ActionSwapIcon value={revealed !== null ? "hide" : "show"} animation="blur" className="size-4">
                    {revealed !== null ? <EyeOff aria-hidden /> : <Eye aria-hidden />}
                  </ActionSwapIcon>
                </IconButton>
                <IconButton label="Copy value" onClick={copy}>
                  <ActionSwapIcon value={copied ? "copied" : "copy"} animation="blur" className="size-4">
                    {copied ? <Check aria-hidden /> : <Copy aria-hidden />}
                  </ActionSwapIcon>
                </IconButton>
              </div>
            </section>

            <RotationSection
              secret={secret}
              provider={provider}
              mode={mode}
              busy={busy}
              onMode={setMode}
              onProviderRotate={rotateWithProvider}
              onRevoke={(byHand) =>
                act(
                  "retire",
                  async () => {
                    await api.revokePreviousWithProvider(secret.id, byHand);
                  },
                  byHand ? "Old key marked retired" : `Revoked the old key at ${provider?.connectionName}`,
                )
              }
              onStage={(value, note) =>
                act("stage", () => api.stageSecret(secret.id, value, note), "New value staged").then(
                  (ok) => ok && setMode("idle"),
                )
              }
              onLeaked={(value) =>
                act(
                  "leaked",
                  () => api.rotateLeakedSecret(secret.id, value),
                  `${secret.name} replaced everywhere. Revoke the leaked value with its provider now.`,
                ).then((ok) => {
                  if (!ok) return;
                  setMode("idle");
                  pushIfTargets();
                })
              }
              onPromote={() =>
                act("promote", () => api.promoteSecret(secret.id), `${secret.name} now uses the new value`).then((ok) => {
                  if (ok) pushIfTargets();
                })
              }
              onCancel={() => act("cancel", () => api.cancelStagedSecret(secret.id), "Staged value discarded")}
              onRollBack={() => act("rollback", () => api.rollBackSecret(secret.id), "Back to the previous value")}
              onRetire={() => act("retire", () => api.retirePreviousSecret(secret.id), "Previous value retired")}
            />

            <SecretIntegrationsSection
              secretId={secret.id}
              secretName={secret.name}
              integrations={integrations}
              onChanged={() => {
                load();
                onChanged();
              }}
            />

            <section aria-labelledby="used-in-title" className="flex flex-col gap-2">
              <h3 id="used-in-title" className="text-xs font-medium text-muted-foreground">
                Used in
              </h3>
              {uses === 0 ? (
                <p className="text-muted-foreground">Not linked anywhere yet.</p>
              ) : (
                <ul className="flex flex-col divide-y rounded-2xl bg-muted/40">
                  {secret.usedIn.map((u) => (
                    <li key={`${u.environmentId}/${u.key}`}>
                      <button
                        type="button"
                        onClick={() => onOpenEnvironment(u.projectId, u.environmentId)}
                        className="flex w-full items-center gap-3 rounded-2xl px-3 py-2 text-left outline-none hover:bg-muted focus-visible:ring-2 focus-visible:ring-ring"
                      >
                        <span className="min-w-0 flex-1 truncate">
                          {u.project}
                          <span className="text-muted-foreground">/{u.environment}</span>
                        </span>
                        <span className="truncate font-mono text-[12px] text-muted-foreground">{u.key}</span>
                      </button>
                    </li>
                  ))}
                </ul>
              )}
            </section>

            <section aria-labelledby="history-title" className="flex flex-col gap-2">
              <h3 id="history-title" className="flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
                <History aria-hidden className="size-3.5" />
                History
              </h3>
              <ol className="flex flex-col divide-y rounded-2xl bg-muted/40">
                {secret.versions.map((v) => (
                  <li key={v.id} className="flex items-center gap-3 px-3 py-2">
                    <span
                      className={cn(
                        "w-18 shrink-0 rounded-full px-2 py-0.5 text-center text-[11px] font-medium",
                        v.state === "current" && "bg-primary/10 text-primary",
                        v.state === "pending" && "bg-foreground/10 text-foreground",
                        v.state === "previous" && "bg-muted text-foreground",
                        v.state === "retired" && "text-muted-foreground",
                      )}
                    >
                      {STATE_LABEL[v.state]}
                    </span>
                    <span className="min-w-0 flex-1 truncate" title={formatDate(v.createdAt)}>
                      {daysAgo(v.createdAt)}
                      {v.note && <span className="text-muted-foreground"> · {v.note}</span>}
                      {v.wiped && <span className="text-muted-foreground"> · value wiped</span>}
                    </span>
                    {(v.state === "retired" || v.state === "previous") && !v.wiped && !secret.staged && (
                      <Button
                        variant="ghost"
                        size="sm"
                        disabled={busy !== null}
                        onClick={() =>
                          act("restore", () => api.restoreSecretVersion(secret.id, v.id), "Earlier value restored")
                        }
                      >
                        <RotateCcw aria-hidden />
                        Restore
                      </Button>
                    )}
                  </li>
                ))}
              </ol>
            </section>

            <SettingsSection
              secret={secret}
              busy={busy}
              onSave={(name, days) =>
                act("settings", () => api.updateSecretSettings(secret.id, name, days), "Saved")
              }
              onDelete={() => setConfirmDelete(true)}
            />
          </div>
        )}
      </SurfaceDialog>
      <ConfirmDialog
        open={confirmDelete}
        busy={busy === "delete"}
        title={`Delete ${secret?.name}?`}
        description={
          uses > 0
            ? `The ${uses === 1 ? "variable" : `${uses} variables`} linked to it keep the current value as their own. Its history is wiped.`
            : "Its value and history are wiped on every synced device."
        }
        confirmLabel="Delete"
        onConfirm={async () => {
          if (!secret) return;
          const ok = await act("delete", () => api.deleteSecret(secret.id), `Deleted ${secret.name}`);
          setConfirmDelete(false);
          if (ok) onOpenChange(false);
        }}
        onCancel={() => setConfirmDelete(false)}
      />
    </>
  );
}

function RotationSection({
  secret,
  provider,
  mode,
  busy,
  onMode,
  onProviderRotate,
  onRevoke,
  onStage,
  onLeaked,
  onPromote,
  onCancel,
  onRollBack,
  onRetire,
}: {
  secret: SecretDetail;
  provider: SecretIntegrations["provider"];
  mode: Mode;
  busy: string | null;
  onMode: (mode: Mode) => void;
  onProviderRotate: (leaked: boolean, pasted?: string | null) => void;
  onRevoke: (byHand: boolean) => void;
  onStage: (value: string | null, note: string | null) => void;
  onLeaked: (value: string | null) => void;
  onPromote: () => void;
  onCancel: () => void;
  onRollBack: () => void;
  onRetire: () => void;
}) {
  if (provider?.guided && (mode === "guided" || mode === "leaked")) {
    return (
      <GuidedForm
        provider={provider}
        leaked={mode === "leaked"}
        busy={busy}
        onCancel={() => onMode("idle")}
        onSubmit={(value) => onProviderRotate(mode === "leaked", value)}
      />
    );
  }
  if (mode === "leaked" && provider) {
    return (
      <Banner
        title="Replace a leaked key"
        body={`dotsafe gets a new key from ${provider.connectionName}, makes it live everywhere, and revokes the leaked one straight away.`}
      >
        <Button variant="ghost" onClick={() => onMode("idle")} disabled={busy !== null}>
          Cancel
        </Button>
        <Button variant="destructive" onClick={() => onProviderRotate(true)} disabled={busy !== null}>
          {busy === "leaked" && <Spinner />}
          Replace and revoke now
        </Button>
      </Banner>
    );
  }
  if (mode !== "idle") {
    return <RotateForm leaked={mode === "leaked"} busy={busy} onCancel={() => onMode("idle")} onStage={onStage} onLeaked={onLeaked} />;
  }
  if (secret.staged) {
    return (
      <Banner
        title="A new value is staged"
        body="Linked variables still use the current value. Update it wherever else it's needed, then promote it."
      >
        <Button variant="ghost" onClick={onCancel} disabled={busy !== null}>
          {busy === "cancel" && <Spinner />}
          Discard
        </Button>
        <Button onClick={onPromote} disabled={busy !== null}>
          {busy === "promote" && <Spinner />}
          Promote
        </Button>
      </Banner>
    );
  }
  if (secret.hasPrevious && provider?.manualRevoke) {
    return (
      <Banner
        title="The previous key still works"
        body={`${provider.serviceLabel} keys can only be revoked in its dashboard. Once nothing uses the old one, revoke it there and mark it retired here.`}
      >
        <Button variant="ghost" onClick={onRollBack} disabled={busy !== null}>
          {busy === "rollback" && <Spinner />}
          Roll back
        </Button>
        <Button variant="ghost" onClick={() => api.openProviderConsole(provider.serviceId)}>
          <ExternalLink aria-hidden />
          Open {provider.serviceLabel}
        </Button>
        <Button variant="outline" onClick={() => onRevoke(true)} disabled={busy !== null}>
          {busy === "retire" && <Spinner />}
          Mark retired
        </Button>
      </Banner>
    );
  }
  if (secret.hasPrevious) {
    return (
      <Banner
        title="The previous value is still valid"
        body={
          provider
            ? `Once nothing uses it, revoke it at ${provider.connectionName}.`
            : "Once nothing uses it, revoke it with its provider and mark it retired here."
        }
      >
        <Button variant="ghost" onClick={onRollBack} disabled={busy !== null}>
          {busy === "rollback" && <Spinner />}
          Roll back
        </Button>
        {provider && (
          <Button variant="ghost" onClick={() => onRevoke(true)} disabled={busy !== null}>
            I revoked it
          </Button>
        )}
        <Button variant="outline" onClick={provider ? () => onRevoke(false) : onRetire} disabled={busy !== null}>
          {busy === "retire" && <Spinner />}
          {provider ? "Revoke old key" : "Mark retired"}
        </Button>
      </Banner>
    );
  }
  return (
    <div className="flex items-center justify-end gap-2">
      <Button variant="ghost" className="text-destructive hover:text-destructive" onClick={() => onMode("leaked")}>
        <ShieldAlert aria-hidden />
        It leaked…
      </Button>
      {provider?.guided ? (
        <Button onClick={() => onMode("guided")} disabled={busy !== null}>
          <RotateCcw aria-hidden />
          New key from {provider.serviceLabel}…
        </Button>
      ) : provider ? (
        <>
          <Button variant="ghost" onClick={() => onMode("rotate")} disabled={busy !== null}>
            Type a value…
          </Button>
          <Button onClick={() => onProviderRotate(false)} disabled={busy !== null}>
            {busy === "provider" ? <Spinner /> : <RotateCcw aria-hidden />}
            New key from {provider.connectionName}
          </Button>
        </>
      ) : (
        <Button onClick={() => onMode("rotate")}>
          <RotateCcw aria-hidden />
          Rotate…
        </Button>
      )}
    </div>
  );
}

/** Rotating with a guided provider: make the key in its console, paste it
 * here, and dotsafe checks it, makes it live and pushes it. */
function GuidedForm({
  provider,
  leaked,
  busy,
  onCancel,
  onSubmit,
}: {
  provider: NonNullable<SecretIntegrations["provider"]>;
  leaked: boolean;
  busy: string | null;
  onCancel: () => void;
  onSubmit: (value: string) => void;
}) {
  const [value, setValue] = useState("");
  return (
    <form
      onSubmit={(e) => {
        e.preventDefault();
        if (value.trim()) onSubmit(value);
      }}
      className="flex flex-col gap-3 rounded-2xl bg-muted/60 p-3.5"
    >
      <div>
        <p className="font-medium">{leaked ? "Replace a leaked key" : `New key from ${provider.serviceLabel}`}</p>
        <p className="mt-0.5 text-[13px] text-muted-foreground">
          {provider.serviceLabel} keys are made in its console. Make one there and paste it here. dotsafe checks it works
          before anything uses it
          {leaked && !provider.manualRevoke ? ", then switches the leaked one off." : "."}
        </p>
      </div>
      <Button
        type="button"
        variant="outline"
        className="self-start bg-card"
        onClick={() => api.openProviderConsole(provider.serviceId).catch((e) => toast.error(message(e)))}
      >
        <ExternalLink aria-hidden />
        Open {provider.serviceLabel}
      </Button>
      <Field label="New key" htmlFor="guided-key">
        <Input
          id="guided-key"
          type="password"
          autoComplete="off"
          className="h-9 bg-card"
          placeholder="Paste the key you made"
          value={value}
          onChange={(e) => setValue(e.target.value)}
        />
      </Field>
      <SurfaceFooter className="pt-0">
        <Button type="button" variant="ghost" onClick={onCancel} disabled={busy !== null}>
          Cancel
        </Button>
        <Button type="submit" variant={leaked ? "destructive" : "default"} disabled={busy !== null || !value.trim()}>
          {busy !== null && <Spinner />}
          {leaked ? "Replace now" : "Check and use it"}
        </Button>
      </SurfaceFooter>
    </form>
  );
}

function Banner({ title, body, children }: { title: string; body: string; children: React.ReactNode }) {
  return (
    <div role="status" className="flex flex-col gap-3 rounded-2xl bg-muted/60 p-3.5">
      <div>
        <p className="font-medium">{title}</p>
        <p className="mt-0.5 text-[13px] text-muted-foreground">{body}</p>
      </div>
      <div className="flex justify-end gap-2">{children}</div>
    </div>
  );
}

function RotateForm({
  leaked,
  busy,
  onCancel,
  onStage,
  onLeaked,
}: {
  leaked: boolean;
  busy: string | null;
  onCancel: () => void;
  onStage: (value: string | null, note: string | null) => void;
  onLeaked: (value: string | null) => void;
}) {
  const [value, setValue] = useState("");
  const [note, setNote] = useState("");
  const [generate, setGenerate] = useState(false);

  function submit(e: FormEvent) {
    e.preventDefault();
    const newValue = generate ? null : value;
    if (leaked) onLeaked(newValue);
    else onStage(newValue, note.trim() || null);
  }

  const ready = generate || value.length > 0;
  return (
    <form onSubmit={submit} className="flex flex-col gap-3 rounded-2xl bg-muted/60 p-3.5">
      <div>
        <p className="font-medium">{leaked ? "Replace a leaked value" : "Rotate"}</p>
        <p className="mt-0.5 text-[13px] text-muted-foreground">
          {leaked
            ? "The new value goes live everywhere at once, and the old one is retired with no overlap."
            : "The new value is staged first. Linked variables switch over when you promote it."}
        </p>
      </div>
      <Field label="New value" htmlFor="new-value">
        <div className="flex gap-2">
          <Input
            id="new-value"
            type="password"
            autoFocus
            autoComplete="off"
            className="h-9 bg-card"
            placeholder={generate ? "A random value will be generated" : "Paste the new value"}
            disabled={generate}
            value={generate ? "" : value}
            onChange={(e) => setValue(e.target.value)}
          />
          <Button
            type="button"
            variant={generate ? "secondary" : "outline"}
            className="h-9 bg-card"
            aria-pressed={generate}
            onClick={() => setGenerate((g) => !g)}
          >
            <Sparkles aria-hidden />
            Generate
          </Button>
        </div>
      </Field>
      {!leaked && (
        <Field label="Note (optional)" htmlFor="rotate-note">
          <Input
            id="rotate-note"
            className="h-9 bg-card"
            placeholder="Quarterly rotation"
            value={note}
            onChange={(e) => setNote(e.target.value)}
          />
        </Field>
      )}
      <SurfaceFooter className="pt-0">
        <Button type="button" variant="ghost" onClick={onCancel} disabled={busy !== null}>
          Cancel
        </Button>
        <Button type="submit" variant={leaked ? "destructive" : "default"} disabled={busy !== null || !ready}>
          {busy !== null && <Spinner />}
          {leaked ? "Replace now" : "Stage new value"}
        </Button>
      </SurfaceFooter>
    </form>
  );
}

function SettingsSection({
  secret,
  busy,
  onSave,
  onDelete,
}: {
  secret: SecretDetail;
  busy: string | null;
  onSave: (name: string, days: number | null) => void;
  onDelete: () => void;
}) {
  const [name, setName] = useState(secret.name);
  const policy = POLICIES.find((p) => p.days === secret.rotateEveryDays)?.value ?? "off";

  return (
    <section aria-labelledby="secret-settings-title" className="flex flex-col gap-3">
      <h3 id="secret-settings-title" className="text-xs font-medium text-muted-foreground">
        Settings
      </h3>
      <form
        className="flex gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          onSave(name, secret.rotateEveryDays);
        }}
      >
        <Input
          aria-label="Secret name"
          className="h-9 font-mono text-[12.5px] md:text-[12.5px]"
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
        <Button type="submit" variant="outline" className="h-9" disabled={busy !== null || name.trim() === secret.name || !name.trim()}>
          Rename
        </Button>
      </form>
      <div className="flex flex-col gap-1.5">
        <p className="text-xs text-muted-foreground">Remind me to rotate every</p>
        <Tabs
          value={policy}
          onValueChange={(value) => onSave(secret.name, POLICIES.find((p) => p.value === value)?.days ?? null)}
        >
          <TabsList aria-label="Rotation reminder" className="w-full bg-muted p-0.5">
            {POLICIES.map((p) => (
              <TabsTrigger key={p.value} value={p.value} className="flex-1 py-1 text-[13px]">
                {p.label}
              </TabsTrigger>
            ))}
          </TabsList>
        </Tabs>
      </div>
      <div className="flex justify-end">
        <Button variant="ghost" className="text-destructive hover:text-destructive" onClick={onDelete}>
          Delete secret…
        </Button>
      </div>
    </section>
  );
}
