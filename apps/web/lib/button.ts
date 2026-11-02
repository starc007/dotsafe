import { cn } from '@/lib/cn';

export type ButtonVariant = 'primary' | 'outline' | 'ghost';

/** The pill button look, shared by links (`ButtonLink`) and real buttons (`BuyButton`). */
export function buttonClass(variant: ButtonVariant, className?: string) {
  return cn(
    'inline-flex h-10 items-center justify-center gap-2 rounded-full px-5 text-sm font-medium transition-[background-color,color,border-color,transform] duration-200 outline-none focus-visible:ring-2 focus-visible:ring-ds-accent/50 active:scale-[0.98] disabled:pointer-events-none [&_svg]:size-4',
    variant === 'primary' && 'bg-ds-primary text-ds-primary-fg hover:bg-ds-primary/85',
    variant === 'outline' && 'border border-ds-line bg-white/70 text-ds-fg backdrop-blur-sm hover:border-ds-fg/20',
    variant === 'ghost' && 'text-ds-muted hover:text-ds-fg',
    className,
  );
}
