import type { ReactNode } from "react";
import { motion } from "motion/react";
import { ShieldCheck } from "lucide-react";
import { base } from "@/lib/motion";
import { cn } from "@/lib/utils";
import { DragRegion } from "@/components/DragRegion";

/** Layout for the pre-vault screens: a draggable window with one centered column. */
export function Centered({ title, subtitle, children }: { title: string; subtitle?: ReactNode; children: ReactNode }) {
  return (
    <div className="relative flex h-full items-center justify-center overflow-y-auto bg-muted/40 p-6">
      <DragRegion className="fixed inset-x-0 top-0 h-12" />
      <motion.main
        className="w-full max-w-[380px]"
        initial={{ opacity: 0, y: 6 }}
        animate={{ opacity: 1, y: 0 }}
        transition={base}
      >
        <div className="mb-6 flex flex-col items-center text-center">
          <div className="mb-4 flex size-11 items-center justify-center rounded-xl bg-primary text-primary-foreground shadow-sm">
            <ShieldCheck aria-hidden className="size-5" strokeWidth={2.2} />
          </div>
          <h1 className="text-lg font-semibold tracking-tight">{title}</h1>
          {subtitle && <p className="mt-1.5 leading-relaxed text-muted-foreground">{subtitle}</p>}
        </div>
        {children}
      </motion.main>
    </div>
  );
}

/** The bordered panel the pre-vault forms sit in. */
export function Panel({ children, className }: { children: ReactNode; className?: string }) {
  return <div className={cn("flex flex-col gap-4 rounded-xl border bg-card p-5 shadow-sm", className)}>{children}</div>;
}
