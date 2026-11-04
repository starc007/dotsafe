import { docsContent } from '@/lib/docs-content';
import { DocsMarkdown } from '@/components/docs/DocsMarkdown';

/** The privacy policy and terms: rendered from `public/<href>.md` like the
 * docs, so each is also served as Markdown at `<href>.md`. */
export function LegalPage({ href }: { href: string }) {
  const page = docsContent(href);
  return (
    <article className="mx-auto w-full max-w-2xl px-4 pt-32 pb-24 sm:px-6 sm:pt-40">
      <p className="text-sm font-medium text-ds-accent">Legal</p>
      <h1 className="mt-3 text-[30px] leading-[1.15] font-medium tracking-[-0.025em] text-balance sm:text-[36px]">
        {page.title}
      </h1>
      <p className="mt-3 text-base leading-relaxed text-pretty text-ds-muted">{page.description}</p>
      <div className="docs-prose mt-10">
        <DocsMarkdown>{page.body}</DocsMarkdown>
      </div>
    </article>
  );
}
