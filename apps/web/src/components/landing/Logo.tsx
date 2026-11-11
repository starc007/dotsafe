import { cn } from '@/lib/cn';

/** The mark: a dot and a stem, kept apart, read together as a d. Drawn in
 * the current text colour. */
export function LogoMark({ className }: { className?: string }) {
  return (
    <svg viewBox="11 8 41 45" aria-hidden className={cn('h-[0.73em] w-auto fill-current', className)}>
      <circle cx="25" cy="39" r="14" />
      <rect x="44" y="8" width="8" height="45" rx="4" />
    </svg>
  );
}

/** The mark with the name beside it. */
export function Logo({ className }: { className?: string }) {
  return (
    <span className={cn('inline-flex items-center gap-2 text-[15px] font-medium tracking-tight', className)}>
      <LogoMark />
      dotsafe
    </span>
  );
}
