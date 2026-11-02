import type { ReactNode } from 'react';
import { Reveal } from '@/components/landing/Reveal';

/** A section's label, then its claim: a dark title and a grey continuation at
 * the same size, read as one sentence. */
export function SectionHeading({ eyebrow, title, children }: { eyebrow: string; title: string; children: ReactNode }) {
  return (
    <Reveal className="max-w-2xl">
      <p className="text-sm text-ds-muted">{eyebrow}</p>
      <h2 className="mt-3 text-[22px] leading-[1.3] font-medium tracking-[-0.005em] text-balance sm:text-2xl">
        {title} <span className="text-ds-muted">{children}</span>
      </h2>
    </Reveal>
  );
}
