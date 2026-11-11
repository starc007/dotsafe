import { cn } from '@/lib/cn';

/** A dropdown panel floating over the page: it fades in and drops 4px, and
 * closing runs the same steps back. Hidden from clicks and focus while closed. */
export function menuPanelClass(open: boolean, className?: string) {
  return cn(
    'absolute z-30 rounded-2xl border border-ds-line bg-ds-card shadow-[0_8px_30px_-12px_rgb(0_0_0/0.18)]',
    'transition-[opacity,translate,visibility] duration-200 ease-[cubic-bezier(0.16,1,0.3,1)] motion-reduce:translate-y-0 motion-reduce:transition-[opacity,visibility]',
    open ? 'visible translate-y-0 opacity-100' : 'invisible -translate-y-1 opacity-0',
    className,
  );
}
