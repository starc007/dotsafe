import type { ReactNode } from "react";
import { Trash2, X } from "lucide-react";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { Spinner } from "@/components/ui/spinner";
import {
  SurfaceBody,
  SurfaceFooter,
  SurfaceHeader,
  surfaceDescriptionClass,
  surfaceTitleClass,
} from "@/components/Surface";

/** A controlled confirmation for destructive actions, in the shell style.
 * Cancel is focused first so Enter never deletes by accident. */
export function ConfirmDialog({
  open,
  title,
  description,
  confirmLabel,
  busy,
  onConfirm,
  onCancel,
}: {
  open: boolean;
  title: string;
  description: ReactNode;
  confirmLabel: string;
  busy?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  return (
    <AlertDialog open={open} onOpenChange={(next) => !next && onCancel()}>
      <AlertDialogContent className="block max-w-sm gap-0 rounded-3xl bg-shell p-1.5 shadow-2xl ring-0 data-[size=default]:sm:max-w-sm">
        <SurfaceHeader
          icon={<Trash2 aria-hidden className="text-destructive" />}
          action={
            <Button
              variant="ghost"
              size="icon"
              aria-label="Close"
              onClick={onCancel}
              className="rounded-full bg-card hover:bg-card/80"
            >
              <X aria-hidden />
            </Button>
          }
        >
          <AlertDialogTitle className={surfaceTitleClass}>{title}</AlertDialogTitle>
          <p className={surfaceDescriptionClass}>This can't be undone.</p>
        </SurfaceHeader>
        <SurfaceBody>
          <AlertDialogDescription className="text-[13px] leading-relaxed text-muted-foreground">
            {description}
          </AlertDialogDescription>
          <SurfaceFooter>
            <AlertDialogCancel variant="ghost">Cancel</AlertDialogCancel>
            <AlertDialogAction
              variant="danger"
              disabled={busy}
              onClick={(e) => {
                // Stay open until the action finishes; the caller closes it.
                e.preventDefault();
                onConfirm();
              }}
            >
              {busy && <Spinner />}
              {confirmLabel}
            </AlertDialogAction>
          </SurfaceFooter>
        </SurfaceBody>
      </AlertDialogContent>
    </AlertDialog>
  );
}
