import type { ReactNode } from "react";

/** One line in a settings card: label and detail on the left, value or control on the right. */
export function SettingsRow({ label, detail, children }: { label: string; detail?: ReactNode; children: ReactNode }) {
  return (
    <div className="flex items-center gap-4 py-3 first:pt-0 last:pb-0">
      <div className="min-w-0 flex-1">
        <p className="font-medium">{label}</p>
        {detail && <p className="mt-0.5 text-xs text-muted-foreground">{detail}</p>}
      </div>
      <div className="shrink-0 text-muted-foreground">{children}</div>
    </div>
  );
}
