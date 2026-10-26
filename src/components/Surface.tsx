import type { ComponentProps, ReactNode } from "react";
import { cn } from "@/lib/utils";

// The app's card and modal style: a soft gray shell holding a header (icon,
// title, description, optional action) and a raised inner card with the
// content. Radii are concentric: shell 24px - 6px padding = 18px inner.

export function Surface({ className, ...props }: ComponentProps<"section">) {
  return (
    <section
      data-slot="surface"
      className={cn("rounded-3xl bg-shell p-1.5", className)}
      {...props}
    />
  );
}

export function SurfaceHeader({
  icon,
  action,
  className,
  children,
}: {
  icon?: ReactNode;
  action?: ReactNode;
  className?: string;
  /** Title and description, usually SurfaceTitle and SurfaceDescription. */
  children: ReactNode;
}) {
  return (
    <div className={cn("flex items-center gap-3 px-3 pt-2.5 pb-3.5", className)}>
      {icon && (
        <div className="flex size-11 shrink-0 items-center justify-center rounded-xl bg-card text-muted-foreground shadow-xs ring-1 ring-foreground/5 [&_svg]:size-5">
          {icon}
        </div>
      )}
      <div className="flex min-w-0 flex-1 flex-col gap-0.5">{children}</div>
      {action}
    </div>
  );
}

export const surfaceTitleClass = "text-[15px] font-semibold leading-tight tracking-tight text-foreground";
export const surfaceDescriptionClass = "text-[13px] text-muted-foreground";

export function SurfaceTitle({ className, ...props }: ComponentProps<"h2">) {
  return <h2 className={cn(surfaceTitleClass, className)} {...props} />;
}

export function SurfaceDescription({ className, ...props }: ComponentProps<"p">) {
  return <p className={cn(surfaceDescriptionClass, className)} {...props} />;
}

export function SurfaceBody({ className, ...props }: ComponentProps<"div">) {
  return (
    <div
      data-slot="surface-body"
      className={cn("rounded-[18px] bg-card p-5 text-card-foreground", className)}
      {...props}
    />
  );
}

/** Right-aligned actions at the bottom of a SurfaceBody. */
export function SurfaceFooter({ className, ...props }: ComponentProps<"div">) {
  return <div className={cn("flex items-center justify-end gap-2 pt-5", className)} {...props} />;
}
