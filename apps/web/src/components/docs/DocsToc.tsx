import { useEffect, useState } from 'react';
import type { DocsHeading } from '@/lib/docs-content';
import { cn } from '@/lib/cn';

/** "On this page": the page's sections, with the one being read marked. */
export function DocsToc({ headings }: { headings: DocsHeading[] }) {
  const [active, setActive] = useState(headings[0]?.id);

  useEffect(() => {
    const els = headings.map((h) => document.getElementById(h.id)).filter((el): el is HTMLElement => !!el);
    // The section being read is the last heading above a line a third of the way down.
    const update = () => {
      const line = window.innerHeight * 0.3;
      let current = els[0]?.id;
      for (const el of els) if (el.getBoundingClientRect().top <= line) current = el.id;
      // At the very bottom, the last section wins even if its heading never reaches the line.
      if (window.innerHeight + window.scrollY >= document.body.scrollHeight - 4) current = els.at(-1)?.id ?? current;
      setActive(current);
    };
    update();
    window.addEventListener('scroll', update, { passive: true });
    return () => window.removeEventListener('scroll', update);
  }, [headings]);

  if (headings.length < 2) return null;
  return (
    <nav aria-label="On this page" className="text-sm">
      <p className="text-xs font-medium text-ds-muted">On this page</p>
      <ul className="mt-3 flex flex-col gap-2 border-l border-ds-line">
        {headings.map((h) => (
          <li key={h.id}>
            <a
              href={`#${h.id}`}
              aria-current={active === h.id ? 'location' : undefined}
              className={cn(
                '-ml-px block border-l-2 pl-3 leading-snug transition-colors duration-200',
                active === h.id
                  ? 'border-ds-primary text-ds-fg'
                  : 'border-transparent text-ds-muted hover:text-ds-fg',
              )}
            >
              {h.text}
            </a>
          </li>
        ))}
      </ul>
    </nav>
  );
}
