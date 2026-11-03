import type { ReactNode } from 'react';
import { docsPage } from '@/lib/docs';
import { DocsPager } from '@/components/docs/DocsPager';

/** A docs page: its title and description from lib/docs.ts, the body styled
 * by `.docs-prose` (global.css), and links to the pages around it. */
export function DocsArticle({ href, children }: { href: string; children: ReactNode }) {
  const page = docsPage(href);
  return (
    <article className="min-w-0">
      <h1 className="text-[28px] leading-[1.2] font-medium tracking-[-0.02em] text-balance sm:text-[32px]">
        {page.title}
      </h1>
      <p className="mt-3 text-[15px] leading-relaxed text-pretty text-ds-muted">{page.description}</p>
      <div className="docs-prose mt-10">{children}</div>
      <DocsPager href={href} />
    </article>
  );
}
