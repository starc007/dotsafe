import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";
import { SquareTerminal } from "lucide-react";
import { api, message, type CliStatus } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { Spinner } from "@/components/ui/spinner";
import { isMac } from "@/lib/platform";
import { Surface, SurfaceBody, SurfaceDescription, SurfaceFooter, SurfaceHeader, SurfaceTitle } from "@/components/Surface";

/** Installs the CLI that ships inside the app, so agents can run `dotsafe mcp`. */
export function CliInstallCard() {
  const [status, setStatus] = useState<CliStatus | null>(null);
  const [busy, setBusy] = useState(false);

  const load = useCallback(async () => {
    try {
      setStatus(await api.cliStatus());
    } catch (e) {
      toast.error(message(e));
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  async function install() {
    setBusy(true);
    try {
      if (await api.installCli()) toast.success("Command-line tool installed");
      await load();
    } catch (e) {
      toast.error(message(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Surface aria-labelledby="cli-title">
      <SurfaceHeader icon={<SquareTerminal aria-hidden />}>
        <SurfaceTitle id="cli-title">Command-line tool</SurfaceTitle>
        <SurfaceDescription>
          {status === null
            ? "Checking…"
            : status.installed
              ? `Installed at ${status.path}`
              : "Needed for agents and the dotsafe CLI"}
        </SurfaceDescription>
      </SurfaceHeader>
      <SurfaceBody>
        {status === null ? (
          <Skeleton aria-hidden className="h-5 w-2/3" />
        ) : status.installed ? (
          <p className="text-muted-foreground">
            <code className="font-mono">dotsafe</code> is ready in your terminal. It updates with the app.
          </p>
        ) : !status.available ? (
          <p className="text-muted-foreground">
            {isMac
              ? "This build doesn't include the command-line tool. The release app from dotsafe.dev does."
              : "The .deb package installs the command-line tool with the app. With the AppImage, download it from dotsafe.dev."}
          </p>
        ) : (
          <p className="text-muted-foreground">
            {status.otherInstall
              ? `Another dotsafe is at ${status.path}. Installing replaces it with the one in this app, which updates with it.`
              : `Adds dotsafe to ${status.path}, so your terminal and agents can run it. macOS asks for your password.`}
          </p>
        )}
        {status?.available && !status.installed && (
          <SurfaceFooter>
            <Button onClick={install} disabled={busy}>
              {busy && <Spinner />}
              {status.otherInstall ? "Replace" : "Install"}
            </Button>
          </SurfaceFooter>
        )}
      </SurfaceBody>
    </Surface>
  );
}
