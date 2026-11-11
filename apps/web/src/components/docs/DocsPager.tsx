import { ArrowLeft, ArrowRight } from 'lucide-react';
import { docsNeighbours } from '@/lib/docs';

/** Previous and next page, in the sidebar's order. */
export function DocsPager({ href }: { href: string }) {
  const { prev, next } = docsNeighbours(href);
  return (
    <nav aria-label="More docs" className="mt-16 grid gap-3 sm:grid-cols-2">
      {prev ? (
        <a
          href={prev.href}
          className="rounded-2xl border border-ds-line p-4 transition-colors duration-200 hover:border-ds-fg/20"
        >
          <span className="flex items-center gap-1.5 text-xs text-ds-muted">
            <ArrowLeft aria-hidden className="size-3.5" /> Previous
          </span>
          <span className="mt-1 block text-sm font-medium">{prev.title}</span>
        </a>
      ) : (
        <span className="hidden sm:block" />
      )}
      {next && (
        <a
          href={next.href}
          className="rounded-2xl border border-ds-line p-4 text-right transition-colors duration-200 hover:border-ds-fg/20"
        >
          <span className="flex items-center justify-end gap-1.5 text-xs text-ds-muted">
            Next <ArrowRight aria-hidden className="size-3.5" />
          </span>
          <span className="mt-1 block text-sm font-medium">{next.title}</span>
        </a>
      )}
    </nav>
  );
}
