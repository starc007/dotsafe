import { useEffect, useState, type FormEvent } from "react";
import { toast } from "sonner";
import { Plug, Plus, Send, X } from "lucide-react";
import { api, message, type ConnectionView, type PushView, type SecretIntegrations, type ServiceView } from "@/lib/api";
import { daysAgo } from "@/lib/time";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Spinner } from "@/components/ui/spinner";
import { Field } from "@/components/Field";
import { IconButton } from "@/components/IconButton";
import { ServiceFields, fieldsComplete } from "@/components/ServiceFields";
import { SurfaceFooter } from "@/components/Surface";

/** Toasts each push's outcome. */
export function reportPushes(pushes: PushView[]) {
  for (const p of pushes) {
    if (p.error) toast.error(`Couldn't push ${p.name}`, { description: p.error });
    else toast.success(`Pushed ${p.name} to ${p.connection}`);
  }
}

type Form = "provider" | "target" | null;

/** Where a secret's values come from (a provider that issues keys) and where
 * they're pushed (deploy targets). */
export function SecretIntegrationsSection({
  secretId,
  secretName,
  integrations,
  onChanged,
}: {
  secretId: string;
  secretName: string;
  integrations: SecretIntegrations | null;
  onChanged: () => void;
}) {
  const [connections, setConnections] = useState<ConnectionView[] | null>(null);
  const [services, setServices] = useState<ServiceView[]>([]);
  const [form, setForm] = useState<Form>(null);
  const [busy, setBusy] = useState<string | null>(null);

  useEffect(() => {
    Promise.all([api.listConnections(), api.listServices()])
      .then(([list, catalogue]) => {
        setConnections(list);
        setServices(catalogue);
      })
      .catch(() => setConnections([]));
  }, []);

  async function act(label: string, run: () => Promise<unknown>, success?: string) {
    setBusy(label);
    try {
      await run();
      if (success) toast.success(success);
      onChanged();
    } catch (e) {
      toast.error(message(e));
    } finally {
      setBusy(null);
    }
  }

  async function pushNow() {
    setBusy("push");
    try {
      reportPushes(await api.pushSecret(secretId));
      onChanged();
    } catch (e) {
      toast.error(message(e));
    } finally {
      setBusy(null);
    }
  }

  const providers = connections?.filter((c) => c.kind === "provider") ?? [];
  const targets = connections?.filter((c) => c.kind === "target") ?? [];
  const provider = integrations?.provider ?? null;

  return (
    <section aria-labelledby="integrations-title" className="flex flex-col gap-2">
      <h3 id="integrations-title" className="flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
        <Plug aria-hidden className="size-3.5" />
        Provider and deploy targets
      </h3>
      <div className="flex flex-col divide-y rounded-2xl bg-muted/40">
        <div className="flex items-center gap-3 px-3 py-2">
          <span className="min-w-0 flex-1 truncate">
            {provider ? (
              <>
                {provider.guided ? "Keys you make in " : "New keys from "}
                <span className="font-medium">{provider.connectionName}</span>
                <span className="text-muted-foreground">
                  {provider.connectionName !== provider.serviceLabel && ` · ${provider.serviceLabel}`}
                  {provider.guided && " · checked by dotsafe"}
                </span>
              </>
            ) : (
              <span className="text-muted-foreground">You type or generate new values.</span>
            )}
          </span>
          {provider ? (
            <Button
              variant="ghost"
              size="sm"
              disabled={busy !== null}
              onClick={() => act("provider", () => api.setSecretProvider(secretId, null, {}), "Provider removed")}
            >
              Remove
            </Button>
          ) : (
            <Button
              variant="ghost"
              size="sm"
              disabled={!providers.length || form !== null}
              title={providers.length ? undefined : "Connect a provider in Settings → Integrations first."}
              onClick={() => setForm("provider")}
            >
              Set a provider…
            </Button>
          )}
        </div>
        {integrations?.targets.map((t) => (
          <div key={t.id} className="flex items-center gap-3 px-3 py-2">
            <span className="min-w-0 flex-1 truncate">
              <span className="font-mono text-[12px]">{t.name}</span>
              <span className="text-muted-foreground">
                {" "}
                on {t.connectionName} ·{" "}
                {t.pushedAt === null ? "never pushed" : t.upToDate ? `pushed ${daysAgo(t.pushedAt)}` : "out of date"}
              </span>
            </span>
            <IconButton
              label={`Stop pushing to ${t.connectionName}`}
              disabled={busy !== null}
              onClick={() => act("remove-target", () => api.removeSecretTarget(secretId, t.id), "Target removed")}
            >
              <X aria-hidden />
            </IconButton>
          </div>
        ))}
      </div>
      {form === "provider" && (
        <ProviderForm
          connections={providers}
          services={services}
          onCancel={() => setForm(null)}
          onSave={(connectionId, settings) =>
            act("provider", () => api.setSecretProvider(secretId, connectionId, settings), "Provider set").then(() =>
              setForm(null),
            )
          }
          busy={busy === "provider"}
        />
      )}
      {form === "target" && (
        <TargetForm
          connections={targets}
          services={services}
          defaultName={secretName}
          onCancel={() => setForm(null)}
          onSave={(connectionId, name, settings) =>
            act("target", () => api.addSecretTarget(secretId, connectionId, name, settings), "Target added").then(() =>
              setForm(null),
            )
          }
          busy={busy === "target"}
        />
      )}
      {form === null && (
        <div className="flex justify-end gap-2">
          <Button
            variant="ghost"
            disabled={!targets.length}
            title={targets.length ? undefined : "Connect Vercel or Cloudflare Workers in Settings → Integrations first."}
            onClick={() => setForm("target")}
          >
            <Plus aria-hidden />
            Add a target…
          </Button>
          {!!integrations?.targets.length && (
            <Button variant="outline" onClick={pushNow} disabled={busy !== null}>
              {busy === "push" ? <Spinner /> : <Send aria-hidden />}
              Push now
            </Button>
          )}
        </div>
      )}
    </section>
  );
}

/** A choice of connection, as pill radios. */
function ConnectionPicker({
  connections,
  value,
  onChange,
}: {
  connections: ConnectionView[];
  value: string | null;
  onChange: (id: string) => void;
}) {
  return (
    <div role="radiogroup" aria-label="Connection" className="flex flex-wrap gap-2">
      {connections.map((c) => (
        <button
          key={c.id}
          type="button"
          role="radio"
          aria-checked={c.id === value}
          onClick={() => onChange(c.id)}
          className={cn(
            "rounded-full bg-card px-3 py-1 text-[13px] outline-none transition-colors",
            "hover:bg-card/70 focus-visible:ring-2 focus-visible:ring-ring",
            "aria-checked:bg-primary/10 aria-checked:text-primary",
          )}
        >
          {c.name}
        </button>
      ))}
    </div>
  );
}

function ProviderForm({
  connections,
  services,
  busy,
  onCancel,
  onSave,
}: {
  connections: ConnectionView[];
  services: ServiceView[];
  busy: boolean;
  onCancel: () => void;
  onSave: (connectionId: string, settings: Record<string, string>) => void;
}) {
  const [connectionId, setConnectionId] = useState<string | null>(connections.length === 1 ? connections[0].id : null);
  const [settings, setSettings] = useState<Record<string, string>>({});
  const connection = connections.find((c) => c.id === connectionId);
  const service = services.find((s) => s.id === connection?.service);
  const fields = service?.bindingFields ?? [];

  function submit(e: FormEvent) {
    e.preventDefault();
    if (connectionId) onSave(connectionId, settings);
  }

  return (
    <form onSubmit={submit} className="flex flex-col gap-3 rounded-2xl bg-muted/60 p-3.5">
      <div>
        <p className="font-medium">Set a provider</p>
        <p className="mt-0.5 text-[13px] text-muted-foreground">
          {service && !service.overlap
            ? "Rotating rolls this token: the old one stops working the moment the new one is issued."
            : "Rotating asks the provider for a new key. The old one keeps working until you revoke it here."}
        </p>
      </div>
      <ConnectionPicker connections={connections} value={connectionId} onChange={setConnectionId} />
      <ServiceFields fields={fields} values={settings} idPrefix="provider" className="h-9 bg-card" onChange={setSettings} />
      <SurfaceFooter className="pt-0">
        <Button type="button" variant="ghost" onClick={onCancel} disabled={busy}>
          Cancel
        </Button>
        <Button type="submit" disabled={busy || !connectionId || !fieldsComplete(fields, settings)}>
          {busy && <Spinner />}
          Set provider
        </Button>
      </SurfaceFooter>
    </form>
  );
}

function TargetForm({
  connections,
  services,
  defaultName,
  busy,
  onCancel,
  onSave,
}: {
  connections: ConnectionView[];
  services: ServiceView[];
  defaultName: string;
  busy: boolean;
  onCancel: () => void;
  onSave: (connectionId: string, name: string, settings: Record<string, string>) => void;
}) {
  const [connectionId, setConnectionId] = useState<string | null>(connections.length === 1 ? connections[0].id : null);
  const [name, setName] = useState(/^[A-Za-z_][A-Za-z0-9_]*$/.test(defaultName) ? defaultName : "");
  const [settings, setSettings] = useState<Record<string, string>>({});
  const connection = connections.find((c) => c.id === connectionId);
  const fields = services.find((s) => s.id === connection?.service)?.bindingFields ?? [];

  function submit(e: FormEvent) {
    e.preventDefault();
    if (connectionId) onSave(connectionId, name, settings);
  }

  return (
    <form onSubmit={submit} className="flex flex-col gap-3 rounded-2xl bg-muted/60 p-3.5">
      <div>
        <p className="font-medium">Add a deploy target</p>
        <p className="mt-0.5 text-[13px] text-muted-foreground">The current value is pushed there on every rotation.</p>
      </div>
      <ConnectionPicker connections={connections} value={connectionId} onChange={setConnectionId} />
      <Field label="Variable name there" htmlFor="target-name">
        <Input
          id="target-name"
          className="h-9 bg-card font-mono text-[12.5px] md:text-[12.5px]"
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
      </Field>
      <ServiceFields fields={fields} values={settings} idPrefix="target" className="h-9 bg-card" onChange={setSettings} />
      <SurfaceFooter className="pt-0">
        <Button type="button" variant="ghost" onClick={onCancel} disabled={busy}>
          Cancel
        </Button>
        <Button type="submit" disabled={busy || !connectionId || !name.trim() || !fieldsComplete(fields, settings)}>
          {busy && <Spinner />}
          Add target
        </Button>
      </SurfaceFooter>
    </form>
  );
}
