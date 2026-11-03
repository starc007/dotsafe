'use client';

import Link from 'next/link';
import { usePathname } from 'next/navigation';
import { DOCS } from '@/lib/docs';
import { cn } from '@/lib/cn';

/** The docs index, grouped. The current page gets a filled pill. */
export function DocsSidebar({ onNavigate, className }: { onNavigate?: () => void; className?: string }) {
  const pathname = usePathname();
  return (
    <nav aria-label="Docs" className={cn('flex flex-col gap-7 text-sm', className)}>
      {DOCS.map((group) => (
        <div key={group.title}>
          <p className="px-3 text-xs font-medium text-ds-muted">{group.title}</p>
          <ul className="mt-2 flex flex-col gap-0.5">
            {group.pages.map((page) => {
              const active = pathname === page.href;
              return (
                <li key={page.href}>
                  <Link
                    href={page.href}
                    onClick={onNavigate}
                    aria-current={active ? 'page' : undefined}
                    className={cn(
                      'block rounded-full px-3 py-1.5 transition-colors duration-200',
                      active ? 'bg-ds-shell font-medium text-ds-fg' : 'text-ds-muted hover:text-ds-fg',
                    )}
                  >
                    {page.title}
                  </Link>
                </li>
              );
            })}
          </ul>
        </div>
      ))}
    </nav>
  );
}
