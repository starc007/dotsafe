import type { ShareView } from "@/lib/api";
import { formatDate } from "@/lib/time";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";

const STATUS: Record<ShareView["status"], { label: string; className: string }> = {
  current: { label: "Up to date", className: "bg-primary/20 text-primary-ink" },
  outOfDate: { label: "Out of date", className: "bg-foreground/10 text-foreground" },
  neverPushed: { label: "Not pushed", className: "bg-foreground/10 text-foreground" },
  expired: { label: "Expired", className: "text-muted-foreground" },
  revoked: { label: "Revoked", className: "text-muted-foreground" },
  broken: { label: "Can't push", className: "bg-destructive/10 text-destructive" },
};

/** Days left before a lease runs out at which a share is flagged. */
export const LEASE_WARNING_DAYS = 3;

/** One share in Settings → Access: its environments, whether its snapshot
 * has the vault's current values, when it ends, and a lease warning. */
export function ShareRow({ share, onRevoke }: { share: ShareView; onRevoke: () => void }) {
  const live = share.status !== "revoked" && share.status !== "expired";
  const status = STATUS[share.status];
  const envs = share.environments.map((e) => (e.project ? `${e.project}/${e.environment}` : e.environment)).join(", ");
  const only = share.environments[0]?.only;
  const left = share.leaseDaysLeft;
  const when =
    share.revoked !== null
      ? `revoked ${formatDate(share.revoked)}`
      : live
        ? `until ${formatDate(share.expires)}`
        : `expired ${formatDate(share.expires)}`;

  return (
    <li className={cn("flex items-center gap-3 py-2.5 pr-4 pl-4", !live && "text-muted-foreground")}>
      <span className="min-w-0 flex-1">
        <span className="flex items-center gap-2">
          <span className={cn("truncate font-medium", live && "text-foreground")}>{share.name}</span>
          <span className={cn("shrink-0 rounded-full px-2 py-0.5 text-[11px] font-medium", status.className)}>
            {status.label}
          </span>
        </span>
        <span className="block truncate text-xs text-muted-foreground">
          {share.approve && "Asks each time · "}
          {envs}
          {only && ` · only ${only.join(", ")}`} · {when}
        </span>
        {share.problem && <span className="block truncate text-xs text-destructive">{share.problem}</span>}
        {left !== null && left < LEASE_WARNING_DAYS && (
          <span className="block truncate text-xs text-destructive">
            {left === 0
              ? `Its ${share.approve ? "mailbox" : "snapshot"} runs out today.`
              : `Its ${share.approve ? "mailbox" : "snapshot"} runs out in ${left} ${left === 1 ? "day" : "days"}.`}{" "}
            Push it, or the machine stops getting values.
          </span>
        )}
      </span>
      {live && (
        <Button variant="ghost" className="text-destructive hover:text-destructive" onClick={onRevoke}>
          Revoke
        </Button>
      )}
    </li>
  );
}
