import { DOCS } from '@/lib/docs';
import { cn } from '@/lib/cn';

/** The docs index, grouped. The current page (`href`) gets a lime pill. */
export function DocsSidebar({ href, onNavigate, className }: { href: string; onNavigate?: () => void; className?: string }) {
  return (
    <nav aria-label="Docs" className={cn('flex flex-col gap-7 text-sm', className)}>
      {DOCS.map((group) => (
        <div key={group.title}>
          <p className="px-3 text-xs font-medium text-ds-muted">{group.title}</p>
          <ul className="mt-2 flex flex-col gap-0.5">
            {group.pages.map((page) => {
              const active = href === page.href;
              const Icon = page.icon;
              return (
                <li key={page.href}>
                  <a
                    href={page.href}
                    onClick={onNavigate}
                    aria-current={active ? 'page' : undefined}
                    className={cn(
                      'flex items-center gap-2.5 rounded-full px-3 py-1.5 transition-colors duration-200',
                      active
                        ? 'bg-ds-primary/25 font-medium text-ds-fg'
                        : 'text-ds-muted hover:bg-ds-shell hover:text-ds-fg',
                    )}
                  >
                    <Icon aria-hidden className={cn('size-4 shrink-0', active ? 'text-ds-accent' : 'text-ds-muted/80')} />
                    {page.title}
                  </a>
                </li>
              );
            })}
          </ul>
        </div>
      ))}
    </nav>
  );
}
