import { useCallback, useEffect, useRef, useState, type FormEvent } from "react";
import { toast } from "sonner";
import { Check, Copy, ExternalLink, Eye, EyeOff, KeyRound, RotateCcw, ShieldAlert, Sparkles, TriangleAlert } from "lucide-react";
import { api, message, type SecretDetail, type SecretIntegrations, type VersionState } from "@/lib/api";
import { daysAgo, formatDate } from "@/lib/time";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { Spinner } from "@/components/ui/spinner";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/motion/tabs";
import { ActionSwapIcon, ActionSwapText } from "@/components/motion/action-swap";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { EnvironmentName } from "@/components/EnvironmentName";
import { Field } from "@/components/Field";
import { IconButton } from "@/components/IconButton";
import { SecretIntegrationsSection, reportPushes } from "@/components/SecretIntegrationsSection";
import { SurfaceDialog } from "@/components/SurfaceDialog";
import { SurfaceFooter } from "@/components/Surface";

const MASK = "••••••••••••";
const REVEAL_MS = 30_000;
const POLICIES = [
  { value: "off", days: null },
  { value: "30", days: 30 },
  { value: "90", days: 90 },
  { value: "180", days: 180 },
  { value: "365", days: 365 },
] as const;

const STATE_LABEL: Record<VersionState, string> = {
  pending: "Staged",
  current: "Current",
  previous: "Previous",
  retired: "Retired",
};

/** `guided`: pasting a key made in a guided provider's console. */
type Mode = "idle" | "rotate" | "leaked" | "guided";
type Tab = "overview" | "used" | "history" | "integrations" | "settings";

/** One secret, in tabs: its value and the rotation flow, where it's
 * used, its history, its provider and targets, and its settings. */
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
  const [tab, setTab] = useState<Tab>("overview");
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
    setTab("overview");
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
        title={secret?.name ?? "Secret"}
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
          <Tabs value={tab} onValueChange={(v) => setTab(v as Tab)} className="-mx-5 -my-5 flex flex-col">
            <div className="px-5 pt-5">
              <TabsList aria-label={`${secret.name} sections`} wrapperClassName="w-full" className="w-full bg-muted p-0.5">
                <TabsTrigger value="overview" className="flex-1 py-1 text-[13px]">
                  Overview
                </TabsTrigger>
                <TabsTrigger value="used" className="flex-1 py-1 text-[13px]">
                  Used in {uses}
                </TabsTrigger>
                <TabsTrigger value="history" className="flex-1 py-1 text-[13px]">
                  History
                </TabsTrigger>
                <TabsTrigger value="integrations" className="flex-1 py-1 text-[13px]">
                  Integrations
                </TabsTrigger>
                <TabsTrigger value="settings" className="flex-1 py-1 text-[13px]">
                  Settings
                </TabsTrigger>
              </TabsList>
            </div>
            <div className="max-h-[min(60vh,520px)] min-h-56 overflow-y-auto px-5 pb-5">
              <TabsContent value="overview" className="flex flex-col gap-4">
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

                <dl className="flex flex-col divide-y rounded-2xl bg-muted/40 text-[13px]">
                  <SummaryRow label="Last rotated">
                    {secret.rotatedAt ? daysAgo(secret.rotatedAt) : "Never"}
                    {secret.due === "overdue" && (
                      <span className="rounded-full bg-destructive/10 px-2 py-0.5 text-[11px] font-medium text-destructive">Overdue</span>
                    )}
                    {secret.due === "soon" && (
                      <span className="rounded-full bg-primary/10 px-2 py-0.5 text-[11px] font-medium text-primary">Due soon</span>
                    )}
                  </SummaryRow>
                  <SummaryRow label="Reminder">
                    <button type="button" onClick={() => setTab("settings")} className="rounded-full outline-none hover:underline focus-visible:ring-2 focus-visible:ring-ring">
                      {reminderLabel(secret.rotateEveryDays)}
                    </button>
                  </SummaryRow>
                  <SummaryRow label="New values">
                    <button type="button" onClick={() => setTab("integrations")} className="rounded-full outline-none hover:underline focus-visible:ring-2 focus-visible:ring-ring">
                      {provider ? `From ${provider.connectionName}` : "Typed or generated by you"}
                    </button>
                  </SummaryRow>
                </dl>

                {secret.copies.length > 0 && (
                  <ul aria-label="Unlinked copies" className="flex flex-col divide-y rounded-2xl bg-muted/40 text-[13px]">
                    {secret.copies.map((c) => (
                      <li key={`${c.environmentId}/${c.key}`} className="flex items-center gap-2.5 py-1.5 pr-1.5 pl-3">
                        <TriangleAlert
                          aria-hidden
                          className={cn("size-4 shrink-0", c.outdated ? "text-destructive" : "text-overridden")}
                        />
                        <span className="min-w-0 flex-1 truncate">
                          <EnvironmentName usage={c} />
                          <span className="text-muted-foreground">
                            {c.outdated ? " still has an old value" : " has its own copy, so rotating skips it"}
                          </span>
                        </span>
                        <Button
                          variant="ghost"
                          size="sm"
                          disabled={busy !== null}
                          onClick={() =>
                            act(
                              "link",
                              () => api.linkVariable(c.environmentId, c.key, secret.id),
                              `${c.key} in ${c.environment} now uses ${secret.name}`,
                            )
                          }
                        >
                          Link
                        </Button>
                      </li>
                    ))}
                  </ul>
                )}

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
                  onStage={(value, note, live) =>
                    act(
                      "stage",
                      () => api.stageSecret(secret.id, value, note, live),
                      live ? `${secret.name} now uses the new value` : "New value staged",
                    ).then((ok) => {
                      if (!ok) return;
                      setMode("idle");
                      if (live) pushIfTargets();
                    })
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
              </TabsContent>

              <TabsContent value="used">
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
                            <EnvironmentName usage={u} />
                          </span>
                          <span className="truncate font-mono text-[12px] text-muted-foreground">{u.key}</span>
                        </button>
                      </li>
                    ))}
                  </ul>
                )}
              </TabsContent>

              <TabsContent value="history" className="flex flex-col gap-3">
                <p className="text-[13px] text-muted-foreground">
                  Every value this secret has had, newest first. Values aren't shown here; restore one to make it current
                  again.
                </p>
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
                        Set {daysAgo(v.createdAt)}
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
                {secret.versions.length === 1 && (
                  <p className="text-[13px] text-muted-foreground">Only one value so far. Each rotation adds an entry.</p>
                )}
              </TabsContent>

              <TabsContent value="integrations">
                <SecretIntegrationsSection
                  secretId={secret.id}
                  secretName={secret.name}
                  integrations={integrations}
                  onChanged={() => {
                    load();
                    onChanged();
                  }}
                />
              </TabsContent>

              <TabsContent value="settings">
                <SettingsSection
                  secret={secret}
                  busy={busy}
                  onSave={(name, days) =>
                    act("settings", () => api.updateSecretSettings(secret.id, name, days), "Saved")
                  }
                  onDelete={() => setConfirmDelete(true)}
                />
              </TabsContent>
            </div>
          </Tabs>
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

function reminderLabel(days: number | null) {
  if (days === null) return "Off";
  return days === 365 ? "Every year" : `Every ${days} days`;
}

function SummaryRow({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex items-center gap-3 px-3 py-2">
      <dt className="w-28 shrink-0 text-muted-foreground">{label}</dt>
      <dd className="flex min-w-0 flex-1 items-center gap-2 truncate">{children}</dd>
    </div>
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
  onStage: (value: string | null, note: string | null, live: boolean) => void;
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
    return (
      <RotateForm
        secretId={secret.id}
        leaked={mode === "leaked"}
        busy={busy}
        onCancel={() => onMode("idle")}
        onStage={onStage}
        onLeaked={onLeaked}
      />
    );
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
        It leaked
      </Button>
      {provider?.guided ? (
        <Button onClick={() => onMode("guided")} disabled={busy !== null}>
          <RotateCcw aria-hidden />
          New key from {provider.serviceLabel}
        </Button>
      ) : provider ? (
        <>
          <Button variant="ghost" onClick={() => onMode("rotate")} disabled={busy !== null}>
            Type a value
          </Button>
          <Button onClick={() => onProviderRotate(false)} disabled={busy !== null}>
            {busy === "provider" ? <Spinner /> : <RotateCcw aria-hidden />}
            New key from {provider.connectionName}
          </Button>
        </>
      ) : (
        <Button onClick={() => onMode("rotate")}>
          <RotateCcw aria-hidden />
          Rotate
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
  secretId,
  leaked,
  busy,
  onCancel,
  onStage,
  onLeaked,
}: {
  secretId: string;
  leaked: boolean;
  busy: string | null;
  onCancel: () => void;
  onStage: (value: string | null, note: string | null, live: boolean) => void;
  onLeaked: (value: string | null) => void;
}) {
  const [value, setValue] = useState("");
  const [note, setNote] = useState("");
  // Staging is for values a service needs first, like a database password:
  // linked variables switch over only when it's promoted.
  const [stageFirst, setStageFirst] = useState(false);
  // A generated value is shown in the field, so it's visible until edited away.
  const [visible, setVisible] = useState(false);
  const [generating, setGenerating] = useState(false);

  async function generate() {
    setGenerating(true);
    try {
      setValue(await api.generateSecretValue(secretId));
      setVisible(true);
    } catch (e) {
      toast.error(message(e));
    } finally {
      setGenerating(false);
    }
  }

  function submit(e: FormEvent) {
    e.preventDefault();
    if (leaked) onLeaked(value);
    else onStage(value, note.trim() || null, !stageFirst);
  }

  return (
    <form onSubmit={submit} className="flex flex-col gap-3 rounded-2xl bg-muted/60 p-3.5">
      <div>
        <p className="font-medium">{leaked ? "Replace a leaked value" : "Rotate"}</p>
        <p className="mt-0.5 text-[13px] text-muted-foreground">
          {leaked
            ? "The new value goes live everywhere at once, and the old one is retired with no overlap."
            : stageFirst
              ? "Linked variables keep the current value until you promote the new one."
              : "The new value goes live everywhere it's linked. The old one stays until you retire it."}
        </p>
      </div>
      <Field
        label="New value"
        htmlFor="new-value"
        hint="Paste a key from the service, or generate a random value for ones you control, like signing keys or database passwords."
      >
        <div className="flex gap-2">
          <Input
            id="new-value"
            type={visible ? "text" : "password"}
            autoFocus
            autoComplete="off"
            spellCheck={false}
            className={cn("h-9 bg-card", visible && "font-mono text-[12.5px] md:text-[12.5px]")}
            placeholder="Paste the new value"
            value={value}
            onChange={(e) => setValue(e.target.value)}
          />
          <Button type="button" variant="outline" className="h-9 bg-card" onClick={generate} disabled={generating}>
            {generating ? <Spinner /> : <Sparkles aria-hidden />}
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
      {!leaked && (
        <Label className="flex w-fit cursor-pointer items-center gap-2 font-normal">
          <Checkbox checked={stageFirst} onCheckedChange={(checked) => setStageFirst(checked === true)} />
          Stage it first, to update the service before it goes live
        </Label>
      )}
      <SurfaceFooter className="pt-0">
        <Button type="button" variant="ghost" onClick={onCancel} disabled={busy !== null}>
          Cancel
        </Button>
        <Button type="submit" variant={leaked ? "destructive" : "default"} disabled={busy !== null || !value}>
          {busy !== null && <Spinner />}
          {leaked ? "Replace now" : stageFirst ? "Stage new value" : "Rotate"}
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
    <section aria-label="Settings" className="flex flex-col gap-5">
      <Field label="Name" htmlFor="secret-rename" hint="Shown in Secrets. Linked variables keep their names.">
        <form
          className="flex gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            onSave(name, secret.rotateEveryDays);
          }}
        >
          <Input
            id="secret-rename"
            className="h-9 font-mono text-[12.5px] md:text-[12.5px]"
            value={name}
            onChange={(e) => setName(e.target.value)}
          />
          <Button type="submit" variant="outline" className="h-9" disabled={busy !== null || name.trim() === secret.name || !name.trim()}>
            Rename
          </Button>
        </form>
      </Field>
      <div className="flex items-center justify-between gap-4">
        <div className="min-w-0">
          <Label htmlFor="rotation-reminder" className="text-[13px]">
            Rotation reminder
          </Label>
          <p className="mt-0.5 text-xs text-muted-foreground">A notification when it's time to rotate.</p>
        </div>
        <Select
          value={policy}
          onValueChange={(value) => onSave(secret.name, POLICIES.find((p) => p.value === value)?.days ?? null)}
          disabled={busy !== null}
        >
          <SelectTrigger id="rotation-reminder" className="h-9 w-40 shrink-0">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {POLICIES.map((p) => (
              <SelectItem key={p.value} value={p.value}>
                {reminderLabel(p.days)}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>
      <div className="flex justify-end">
        <Button variant="ghost" className="text-destructive hover:text-destructive" onClick={onDelete}>
          Delete secret
        </Button>
      </div>
    </section>
  );
}
