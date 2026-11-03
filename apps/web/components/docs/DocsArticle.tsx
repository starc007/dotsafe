import type { ReactNode } from 'react';
import { docsEntry } from '@/lib/docs';
import { docsContent } from '@/lib/docs-content';
import { DocsMarkdown } from '@/components/docs/DocsMarkdown';
import { DocsPageActions } from '@/components/docs/DocsPageActions';
import { DocsPager } from '@/components/docs/DocsPager';
import { DocsToc } from '@/components/docs/DocsToc';

/** A docs page, rendered from `public/<href>.md`: its section and title,
 * the Copy page menu, the body, links to the pages around it, and "On this
 * page" beside it on wide screens. `children` go after the body. */
export function DocsArticle({ href, children }: { href: string; children?: ReactNode }) {
  const entry = docsEntry(href);
  const page = docsContent(href);
  return (
    <div className="grid gap-12 xl:grid-cols-[minmax(0,1fr)_11rem]">
      <article className="min-w-0 max-w-2xl">
        <div className="flex items-center justify-between gap-4">
          <p className="text-sm font-medium text-ds-accent">{entry.group}</p>
          <DocsPageActions href={href} markdown={page.markdown} />
        </div>
        <h1 className="mt-3 text-[30px] leading-[1.15] font-medium tracking-[-0.025em] text-balance sm:text-[36px]">
          {page.title}
        </h1>
        <p className="mt-3 text-base leading-relaxed text-pretty text-ds-muted">{page.description}</p>
        <div className="docs-prose mt-10">
          <DocsMarkdown>{page.body}</DocsMarkdown>
        </div>
        {children}
        <DocsPager href={href} />
      </article>
      <aside className="hidden xl:block">
        <div className="sticky top-28">
          <DocsToc headings={page.headings} />
        </div>
      </aside>
    </div>
  );
}
