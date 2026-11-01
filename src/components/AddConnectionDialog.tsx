import { useEffect, useState, type FormEvent } from "react";
import { toast } from "sonner";
import { Plug } from "lucide-react";
import { api, message, type ServiceView } from "@/lib/api";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Spinner } from "@/components/ui/spinner";
import { Field } from "@/components/Field";
import { ServiceFields, fieldsComplete } from "@/components/ServiceFields";
import { SurfaceDialog } from "@/components/SurfaceDialog";
import { SurfaceFooter } from "@/components/Surface";
import { ServiceLogo } from "@/components/ServiceLogo";

const GROUPS = [
  { kind: "provider", title: "Rotate keys with" },
  { kind: "target", title: "Push new values to" },
] as const;

/** Connects an account at a provider or deploy target. The admin token goes
 * to Rust once and is kept as a shared secret. */
export function AddConnectionDialog({
  open,
  services,
  onOpenChange,
  onAdded,
}: {
  open: boolean;
  services: ServiceView[];
  onOpenChange: (open: boolean) => void;
  onAdded: () => void;
}) {
  const [serviceId, setServiceId] = useState<string | null>(null);
  const [name, setName] = useState("");
  const [settings, setSettings] = useState<Record<string, string>>({});
  const [token, setToken] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const service = services.find((s) => s.id === serviceId) ?? null;

  useEffect(() => {
    if (!open) return;
    setServiceId(null);
    setName("");
    setSettings({});
    setToken("");
    setError(null);
  }, [open]);

  function pick(s: ServiceView) {
    setServiceId(s.id);
    setSettings({});
    setName((n) => (n === "" || services.some((x) => x.label === n) ? s.label : n));
  }

  async function submit(e: FormEvent) {
    e.preventDefault();
    if (!service) return;
    setBusy(true);
    setError(null);
    try {
      await api.addConnection(service.id, name, settings, token);
      toast.success(`Connected ${name}`);
      onAdded();
      onOpenChange(false);
    } catch (err) {
      setError(message(err));
    } finally {
      setBusy(false);
    }
  }

  const ready =
    service && name.trim() && (token.trim() || !service.needsToken) && fieldsComplete(service.connectionFields, settings);
  return (
    <SurfaceDialog
      open={open}
      onOpenChange={(o) => !busy && onOpenChange(o)}
      icon={<Plug aria-hidden />}
      title="Add a connection"
      description="dotsafe calls the service from this Mac with your own token. Nothing goes through a dotsafe server."
    >
      <form onSubmit={submit} className="flex flex-col gap-4">
        <div role="radiogroup" aria-label="Service" className="flex flex-col gap-3">
          {GROUPS.map((group) => {
            const members = services.filter((s) => s.kind === group.kind);
            if (members.length === 0) return null;
            return (
              <div key={group.kind} className="flex flex-col gap-1.5">
                <p className="px-1 text-xs font-medium text-muted-foreground">{group.title}</p>
                <div className="grid grid-cols-2 gap-1.5">
                  {members.map((s) => (
                    <button
                      key={s.id}
                      type="button"
                      role="radio"
                      aria-checked={s.id === serviceId}
                      onClick={() => pick(s)}
                      className={cn(
                        "flex min-w-0 items-center gap-2.5 rounded-xl bg-muted/60 p-2 pr-3 text-left outline-none transition-colors",
                        "hover:bg-muted focus-visible:ring-2 focus-visible:ring-ring",
                        "aria-checked:bg-primary/10",
                      )}
                    >
                      <ServiceLogo service={s.id} />
                      <span className="flex min-w-0 flex-col">
                        <span className="truncate text-[13px] font-medium" title={s.label}>
                          {s.label}
                        </span>
                        <span className="truncate text-xs text-muted-foreground">
                          {s.kind === "target" ? "Updated on rotation" : s.guided ? "Checks keys you make" : "Makes new keys"}
                        </span>
                      </span>
                    </button>
                  ))}
                </div>
              </div>
            );
          })}
        </div>
        {service && (
          <>
            <Field label="Name" htmlFor="connection-name" hint="Shown when choosing where a secret comes from or goes.">
              <Input id="connection-name" className="h-9" value={name} onChange={(e) => setName(e.target.value)} />
            </Field>
            <ServiceFields
              fields={service.connectionFields}
              values={settings}
              idPrefix="connection"
              onChange={setSettings}
            />
            {service.needsToken ? (
              <Field label="Admin token" htmlFor="connection-token" hint={service.tokenHelp} error={error}>
                <Input
                  id="connection-token"
                  type="password"
                  autoComplete="off"
                  className="h-9"
                  value={token}
                  aria-invalid={!!error}
                  onChange={(e) => setToken(e.target.value)}
                />
              </Field>
            ) : (
              <p className="text-[13px] text-muted-foreground">{error ?? service.tokenHelp}</p>
            )}
          </>
        )}
        <SurfaceFooter className="pt-1">
          <Button type="button" variant="ghost" onClick={() => onOpenChange(false)} disabled={busy}>
            Cancel
          </Button>
          <Button type="submit" disabled={busy || !ready}>
            {busy && <Spinner />}
            Connect
          </Button>
        </SurfaceFooter>
      </form>
    </SurfaceDialog>
  );
}
