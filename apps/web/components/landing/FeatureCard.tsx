import type { ReactNode } from 'react';
import { cn } from '@/lib/cn';
import { Reveal } from '@/components/landing/Reveal';

/** A feature: its live demo on top, then the claim as one two-tone line. Border only. */
export function FeatureCard({
  title,
  children,
  demo,
  className,
  delay,
}: {
  title: string;
  children: ReactNode;
  demo: ReactNode;
  className?: string;
  delay?: number;
}) {
  return (
    <Reveal delay={delay} className={cn('flex flex-col overflow-hidden rounded-2xl border border-ds-line bg-white', className)}>
      <div className="flex min-h-64 flex-1 items-center justify-center border-b border-ds-line p-5 sm:p-6">{demo}</div>
      <p className="p-5 text-[15px] leading-relaxed text-pretty sm:p-6">
        <span className="font-medium">{title}.</span> <span className="text-ds-muted">{children}</span>
      </p>
    </Reveal>
  );
}
