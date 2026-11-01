import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { toast } from "sonner";
import { ArrowDownToLine, Download } from "lucide-react";
import { api, message, type UpdateProgress, type UpdateView } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { Spinner } from "@/components/ui/spinner";
import { SurfaceDialog } from "@/components/SurfaceDialog";
import { SurfaceFooter } from "@/components/Surface";

/** Shows a newer version and its notes. Nothing downloads until the user
 * clicks; then it downloads, installs and restarts the app. */
export function UpdateDialog({
  update,
  open,
  onOpenChange,
}: {
  update: UpdateView;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const [progress, setProgress] = useState<UpdateProgress | null>(null);
  const installing = progress !== null;

  useEffect(() => {
    const unlisten = listen<UpdateProgress>("update-progress", (e) => setProgress(e.payload));
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  async function install() {
    setProgress({ downloaded: 0, total: null });
    try {
      // Restarts the app when it's done, so this only returns on failure.
      await api.installUpdate();
    } catch (e) {
      setProgress(null);
      toast.error(message(e));
    }
  }

  const percent = progress?.total ? Math.min(100, Math.round((progress.downloaded / progress.total) * 100)) : null;
  return (
    <SurfaceDialog
      open={open}
      onOpenChange={(o) => !installing && onOpenChange(o)}
      icon={<ArrowDownToLine aria-hidden />}
      title={`dotsafe ${update.version} is available`}
      description={`You have ${update.currentVersion}.`}
    >
      <div className="flex flex-col gap-4">
        {update.notes ? (
          <div className="selectable max-h-56 overflow-y-auto rounded-2xl bg-muted/60 px-3.5 py-3 text-[13px] whitespace-pre-wrap">
            {update.notes}
          </div>
        ) : (
          <p className="text-[13px] text-muted-foreground">No release notes for this version.</p>
        )}
        {installing && (
          <div className="flex flex-col gap-1.5" role="status">
            <Progress value={percent ?? 0} aria-label="Download progress" />
            <p className="text-xs text-muted-foreground tabular-nums">
              {percent === null ? "Downloading" : `Downloading ${percent}%`}. dotsafe restarts when it's done.
            </p>
          </div>
        )}
        <SurfaceFooter className="pt-1">
          <Button variant="ghost" onClick={() => onOpenChange(false)} disabled={installing}>
            Later
          </Button>
          <Button onClick={install} disabled={installing}>
            {installing ? <Spinner /> : <Download aria-hidden />}
            Download and restart
          </Button>
        </SurfaceFooter>
      </div>
    </SurfaceDialog>
  );
}
