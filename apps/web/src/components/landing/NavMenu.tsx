import { useCallback, useRef, useState } from 'react';
import { Menu, X } from 'lucide-react';
import { cn } from '@/lib/cn';
import { menuPanelClass } from '@/lib/menu';
import { useDismiss } from '@/lib/use-dismiss';

/** The nav's links on phones, where the bar only has room for the logo and
 * Download: a round button that opens them in a panel under the bar. */
export function NavMenu({ items }: { items: { label: string; href: string }[] }) {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const toggle = useRef<HTMLButtonElement>(null);
  const close = useCallback(() => setOpen(false), []);
  useDismiss(open, close, root, toggle);
  const label = open ? 'Close menu' : 'Menu';

  return (
    <div ref={root} className="sm:hidden">
      <button
        ref={toggle}
        type="button"
        onClick={() => setOpen((o) => !o)}
        aria-expanded={open}
        aria-controls="nav-menu"
        aria-label={label}
        title={label}
        className="relative flex size-9 cursor-pointer items-center justify-center rounded-full text-ds-fg transition-colors duration-200 hover:bg-ds-shell"
      >
        {/* The two icons cross-fade in place, so the button never changes size. */}
        <Menu
          aria-hidden
          className={cn('absolute size-[18px] transition-opacity duration-200 motion-reduce:transition-none', open && 'opacity-0')}
        />
        <X
          aria-hidden
          className={cn('absolute size-[18px] transition-opacity duration-200 motion-reduce:transition-none', !open && 'opacity-0')}
        />
      </button>
      <nav
        id="nav-menu"
        aria-label="Site"
        inert={!open}
        className={menuPanelClass(open, 'inset-x-4 top-full mt-2 p-1.5')}
      >
        <ul className="flex flex-col">
          {items.map((item) => (
            <li key={item.href}>
              <a
                href={item.href}
                onClick={close}
                className="flex rounded-xl px-3 py-2.5 text-sm transition-colors duration-150 outline-none hover:bg-ds-shell focus-visible:bg-ds-shell"
              >
                {item.label}
              </a>
            </li>
          ))}
        </ul>
      </nav>
    </div>
  );
}
