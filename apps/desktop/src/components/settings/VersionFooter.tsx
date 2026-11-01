import { useState } from "react";
import { toast } from "sonner";
import { RefreshCw } from "lucide-react";
import { api, message } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Spinner } from "@/components/ui/spinner";

/** The app's version, and a way to look for a newer one. A found update
 * opens the update dialog, which belongs to the vault screen. */
export function VersionFooter({ version }: { version: string }) {
  const [checking, setChecking] = useState(false);

  async function check() {
    setChecking(true);
    try {
      const update = await api.checkForUpdate();
      if (update) window.dispatchEvent(new CustomEvent("dotsafe:update-found", { detail: update }));
      else toast.success(`dotsafe ${version} is the latest version`);
    } catch (e) {
      toast.error(message(e));
    } finally {
      setChecking(false);
    }
  }

  return (
    <div className="flex items-center justify-center gap-2 pt-2 text-xs text-muted-foreground">
      <span className="tabular-nums">dotsafe {version}</span>
      <Button variant="ghost" size="sm" onClick={check} disabled={checking} className="h-7 text-xs">
        {checking ? <Spinner /> : <RefreshCw aria-hidden />}
        Check for updates
      </Button>
    </div>
  );
}
