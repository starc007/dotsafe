import { useCallback, useRef, useState } from 'react';
import { ChevronDown } from 'lucide-react';
import { DOCS } from '@/lib/docs';
import { cn } from '@/lib/cn';
import { menuPanelClass } from '@/lib/menu';
import { useDismiss } from '@/lib/use-dismiss';
import { DocsSidebar } from '@/components/docs/DocsSidebar';

/** Below `lg` the sidebar folds into a button that names the current page
 * (`href`). The list opens over the page instead of pushing it down. */
export function DocsMobileNav({ href }: { href: string }) {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const toggle = useRef<HTMLButtonElement>(null);
  const close = useCallback(() => setOpen(false), []);
  useDismiss(open, close, root, toggle);
  const current = DOCS.flatMap((g) => g.pages).find((p) => p.href === href);

  return (
    <div ref={root} className="relative lg:hidden">
      <button
        ref={toggle}
        type="button"
        onClick={() => setOpen((o) => !o)}
        aria-expanded={open}
        aria-controls="docs-mobile-nav"
        className="flex h-10 w-full cursor-pointer items-center justify-between rounded-full bg-ds-shell px-4 text-sm font-medium"
      >
        {current?.title ?? 'Docs'}
        <ChevronDown
          aria-hidden
          className={cn(
            'size-4 text-ds-muted transition-transform duration-200 motion-reduce:transition-none',
            open && 'rotate-180',
          )}
        />
      </button>
      <div
        id="docs-mobile-nav"
        inert={!open}
        className={menuPanelClass(open, 'inset-x-0 top-full mt-2 max-h-[min(32rem,calc(100dvh-10rem))] overflow-y-auto overscroll-contain p-2')}
      >
        <DocsSidebar href={href} onNavigate={close} className="gap-5 py-2" />
      </div>
    </div>
  );
}
