import type { ReactNode } from 'react';
import { Info, Lightbulb, TriangleAlert } from 'lucide-react';
import { cn } from '@/lib/cn';

const KINDS = {
  note: { icon: Info, label: 'Note', tone: 'text-ds-accent' },
  tip: { icon: Lightbulb, label: 'Tip', tone: 'text-ds-accent' },
  warning: { icon: TriangleAlert, label: 'Warning', tone: 'text-ds-warn' },
};

/** A `> [!NOTE]`, `[!TIP]` or `[!WARNING]` from the Markdown. Plain
 * blockquotes keep their quote look. */
export function DocsCallout({ kind, children }: { kind?: string; children: ReactNode }) {
  const k = KINDS[kind as keyof typeof KINDS];
  if (!k) return <blockquote>{children}</blockquote>;
  const Icon = k.icon;
  return (
    <aside
      aria-label={k.label}
      className={cn(
        'flex gap-3 rounded-2xl border p-4 text-[14px] leading-relaxed [&_p]:m-0',
        kind === 'warning' ? 'border-ds-warn/30 bg-ds-warn/[0.04]' : 'border-ds-primary/50 bg-ds-primary/[0.08]',
      )}
    >
      <Icon aria-hidden className={cn('mt-[3px] size-4 shrink-0', k.tone)} />
      <div className="min-w-0">{children}</div>
    </aside>
  );
}
