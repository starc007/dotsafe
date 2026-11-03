'use client';

import { useState } from 'react';
import { usePathname } from 'next/navigation';
import { ChevronDown } from 'lucide-react';
import { DOCS } from '@/lib/docs';
import { cn } from '@/lib/cn';
import { DocsSidebar } from '@/components/docs/DocsSidebar';

/** Below `lg` the sidebar folds into a button that names the current page. */
export function DocsMobileNav() {
  const [open, setOpen] = useState(false);
  const pathname = usePathname();
  const current = DOCS.flatMap((g) => g.pages).find((p) => p.href === pathname);
  return (
    <div className="lg:hidden">
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        aria-expanded={open}
        aria-controls="docs-mobile-nav"
        className="flex h-10 w-full cursor-pointer items-center justify-between rounded-full bg-ds-shell px-4 text-sm font-medium"
      >
        {current?.title ?? 'Docs'}
        <ChevronDown
          aria-hidden
          className={cn('size-4 text-ds-muted transition-transform duration-200', open && 'rotate-180')}
        />
      </button>
      {/* Same grid-rows trick as the FAQ: the panel opens to its own height. */}
      <div
        id="docs-mobile-nav"
        className={cn(
          'grid transition-[grid-template-rows] duration-300 ease-[cubic-bezier(0.16,1,0.3,1)] motion-reduce:transition-none',
          open ? 'grid-rows-[1fr]' : 'grid-rows-[0fr]',
        )}
      >
        <div className="overflow-hidden" inert={!open}>
          <DocsSidebar onNavigate={() => setOpen(false)} className="pt-6 pb-2" />
        </div>
      </div>
    </div>
  );
}
