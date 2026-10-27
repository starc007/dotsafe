import { TriangleAlert } from "lucide-react";
import type { VaultHealth } from "@/lib/api";
import { Surface, SurfaceBody, SurfaceDescription, SurfaceHeader, SurfaceTitle } from "@/components/Surface";

/** Explains what the vault's integrity checks found. Shown only when they
 * found something. */
export function IntegrityNotice({ health }: { health: VaultHealth }) {
  if (!health.stale && !health.unreadable) return null;
  return (
    <Surface aria-labelledby="integrity-title" role="alert">
      <SurfaceHeader icon={<TriangleAlert aria-hidden className="text-destructive" />}>
        <SurfaceTitle id="integrity-title">Vault needs a look</SurfaceTitle>
        <SurfaceDescription>dotsafe checks every record it reads. Some didn't pass.</SurfaceDescription>
      </SurfaceHeader>
      <SurfaceBody className="flex flex-col gap-3 text-[13px]">
        {health.stale > 0 && (
          <p>
            <span className="font-medium">{plural(health.stale, "record is", "records are")} older than a copy seen before.</span>{" "}
            <span className="text-muted-foreground">
              Your sync client may still be catching up; wait a minute and lock and unlock again. If it stays, someone
              with access to your storage may have put back an old copy, so check recent changes.
            </span>
          </p>
        )}
        {health.unreadable > 0 && (
          <p>
            <span className="font-medium">{plural(health.unreadable, "record was", "records were")} skipped.</span>{" "}
            <span className="text-muted-foreground">
              They're sealed with the vault's old key, so a removed device probably wrote them after it was removed.
              dotsafe ignores them; nothing in them was applied.
            </span>
          </p>
        )}
      </SurfaceBody>
    </Surface>
  );
}

function plural(n: number, one: string, many: string) {
  return n === 1 ? `1 ${one}` : `${n} ${many}`;
}
