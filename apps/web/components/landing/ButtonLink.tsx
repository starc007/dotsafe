import Link from 'next/link';
import type { ComponentProps } from 'react';
import { cn } from '@/lib/cn';

/** A link styled as a pill button: `primary` for the one main action. */
export function ButtonLink({
  variant = 'outline',
  className,
  ...props
}: ComponentProps<typeof Link> & { variant?: 'primary' | 'outline' | 'ghost' }) {
  return (
    <Link
      className={cn(
        'inline-flex h-10 items-center justify-center gap-2 rounded-full px-5 text-sm font-medium transition-[background-color,color,border-color,transform] duration-200 outline-none focus-visible:ring-2 focus-visible:ring-ds-accent/50 active:scale-[0.98] [&_svg]:size-4',
        variant === 'primary' && 'bg-ds-primary text-ds-primary-fg hover:bg-ds-primary/85',
        variant === 'outline' && 'border border-ds-line bg-white/70 text-ds-fg backdrop-blur-sm hover:border-ds-fg/20',
        variant === 'ghost' && 'text-ds-muted hover:text-ds-fg',
        className,
      )}
      {...props}
    />
  );
}
