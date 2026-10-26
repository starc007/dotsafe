import type { ReactNode } from "react";
import { ShieldCheck } from "lucide-react";
import { DragRegion } from "./ui";

/** Layout for the pre-vault screens: a draggable window with one centered card. */
export function Centered({ title, subtitle, children }: { title: string; subtitle?: ReactNode; children: ReactNode }) {
  return (
    <div className="relative flex h-full items-center justify-center overflow-y-auto bg-surface p-6">
      <DragRegion className="fixed inset-x-0 top-0 h-12" />
      <main className="animate-in w-full max-w-[380px]">
        <div className="mb-6 flex flex-col items-center text-center">
          <div className="mb-4 flex size-11 items-center justify-center rounded-xl bg-primary text-primary-foreground shadow-sm">
            <ShieldCheck aria-hidden className="size-5" strokeWidth={2.2} />
          </div>
          <h1 className="text-lg font-semibold tracking-tight">{title}</h1>
          {subtitle && <p className="mt-1.5 text-[13px] leading-relaxed text-muted-foreground">{subtitle}</p>}
        </div>
        {children}
      </main>
    </div>
  );
}
