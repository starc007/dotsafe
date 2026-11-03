'use client';

import { useEffect, useRef, useState } from 'react';
import { Check, ChevronDown, Copy, FileText, Link2 } from 'lucide-react';
import { cn } from '@/lib/cn';

/** "Copy page" for this docs page, with a menu: copy its Markdown for an AI
 * tool, open the raw .md, or copy a link to it. */
export function DocsPageActions({ href, markdown }: { href: string; markdown: string }) {
  const [open, setOpen] = useState(false);
  const [done, setDone] = useState<'page' | 'link' | null>(null);
  const root = useRef<HTMLDivElement>(null);
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);
  const mdPath = `${href}.md`;

  useEffect(() => () => clearTimeout(timer.current), []);

  // Close on a click outside or Escape, and hand focus back to the toggle.
  useEffect(() => {
    if (!open) return;
    const onPointer = (e: PointerEvent) => {
      if (!root.current?.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        setOpen(false);
        root.current?.querySelector<HTMLButtonElement>('[aria-haspopup]')?.focus();
      }
    };
    document.addEventListener('pointerdown', onPointer);
    document.addEventListener('keydown', onKey);
    root.current?.querySelector<HTMLElement>('[role=menuitem]')?.focus();
    return () => {
      document.removeEventListener('pointerdown', onPointer);
      document.removeEventListener('keydown', onKey);
    };
  }, [open]);

  async function copy(what: 'page' | 'link') {
    setOpen(false);
    try {
      const origin = window.location.origin;
      // Links in the Markdown are root-relative; outside the site they need the host.
      const page = markdown.replace(/\]\(\//g, `](${origin}/`);
      await navigator.clipboard.writeText(what === 'page' ? page : `${origin}${mdPath}`);
      setDone(what);
      clearTimeout(timer.current);
      timer.current = setTimeout(() => setDone(null), 1500);
    } catch {
      // Clipboard blocked: "View as Markdown" still works.
    }
  }

  // Up and down move between the menu's items.
  function onMenuKey(e: React.KeyboardEvent) {
    if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
    e.preventDefault();
    const items = [...(root.current?.querySelectorAll<HTMLElement>('[role=menuitem]') ?? [])];
    const i = items.indexOf(document.activeElement as HTMLElement);
    items[(i + (e.key === 'ArrowDown' ? 1 : items.length - 1)) % items.length]?.focus();
  }

  const item =
    'flex w-full cursor-pointer items-center gap-2.5 rounded-xl px-3 py-2 text-left text-sm outline-none transition-colors duration-150 hover:bg-ds-shell focus-visible:bg-ds-shell [&_svg]:size-4 [&_svg]:text-ds-muted';

  return (
    <div ref={root} className="relative shrink-0">
      <div className="flex h-8 items-center rounded-full border border-ds-line text-sm">
        <button
          type="button"
          onClick={() => copy('page')}
          className="flex h-full cursor-pointer items-center gap-1.5 rounded-l-full pr-2.5 pl-3 transition-colors duration-200 hover:bg-ds-shell"
        >
          {done ? (
            <Check aria-hidden className="size-3.5 text-ds-accent" />
          ) : (
            <Copy aria-hidden className="size-3.5 text-ds-muted" />
          )}
          <span aria-live="polite">{done === 'page' ? 'Copied' : done === 'link' ? 'Link copied' : 'Copy page'}</span>
        </button>
        <span aria-hidden className="h-4 w-px bg-ds-line" />
        <button
          type="button"
          onClick={() => setOpen((o) => !o)}
          aria-haspopup="menu"
          aria-expanded={open}
          aria-label="More ways to use this page"
          title="More"
          className="flex h-full cursor-pointer items-center rounded-r-full pr-2.5 pl-2 transition-colors duration-200 hover:bg-ds-shell"
        >
          <ChevronDown
            aria-hidden
            className={cn('size-3.5 text-ds-muted transition-transform duration-200', open && 'rotate-180')}
          />
        </button>
      </div>
      <div
        role="menu"
        onKeyDown={onMenuKey}
        hidden={!open}
        className="absolute top-full right-0 z-30 mt-2 w-60 rounded-2xl border border-ds-line bg-ds-card p-1.5 shadow-[0_8px_30px_-12px_rgb(0_0_0/0.18)]"
      >
        <button type="button" role="menuitem" className={item} onClick={() => copy('page')}>
          <Copy aria-hidden />
          Copy page for AI
        </button>
        <a role="menuitem" className={item} href={mdPath} target="_blank" rel="noreferrer" onClick={() => setOpen(false)}>
          <FileText aria-hidden />
          View as Markdown
        </a>
        <button type="button" role="menuitem" className={item} onClick={() => copy('link')}>
          <Link2 aria-hidden />
          Copy Markdown link
        </button>
      </div>
    </div>
  );
}
