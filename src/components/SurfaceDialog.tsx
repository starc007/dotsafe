import type { ReactNode } from "react";
import { X } from "lucide-react";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Dialog, DialogClose, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { SurfaceBody, SurfaceHeader, surfaceDescriptionClass, surfaceTitleClass } from "@/components/Surface";

/** A modal in the shell style: icon, title and close button in the gray
 * shell, content in the inner card. Escape and clicking outside close it. */
export function SurfaceDialog({
  open,
  onOpenChange,
  icon,
  title,
  description,
  className,
  children,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  icon: ReactNode;
  title: string;
  description?: ReactNode;
  className?: string;
  children: ReactNode;
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        showCloseButton={false}
        // Without a description, tell Radix there's nothing to describe it.
        {...(!description && { "aria-describedby": undefined })}
        className={cn("block gap-0 rounded-3xl border-0 bg-shell p-1.5 shadow-2xl ring-0 sm:max-w-md", className)}
      >
        <SurfaceHeader
          icon={icon}
          action={
            <DialogClose asChild>
              <Button variant="ghost" size="icon" aria-label="Close" className="rounded-full bg-card hover:bg-card/80">
                <X aria-hidden />
              </Button>
            </DialogClose>
          }
        >
          <DialogTitle className={surfaceTitleClass}>{title}</DialogTitle>
          {description && <DialogDescription className={surfaceDescriptionClass}>{description}</DialogDescription>}
        </SurfaceHeader>
        <SurfaceBody>{children}</SurfaceBody>
      </DialogContent>
    </Dialog>
  );
}
