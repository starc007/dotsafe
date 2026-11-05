import Link from 'next/link';
import { docsContent } from '@/lib/docs-content';
import { contentPageData } from '@/lib/structured-data';
import { DocsMarkdown } from '@/components/docs/DocsMarkdown';
import { FinalCta } from '@/components/landing/FinalCta';
import { JsonLd } from '@/components/landing/JsonLd';

/** A page written in Markdown at `public/<href>.md`, like the docs: the legal
 * pages, the alternatives and the use-case pages. The same file is served as
 * is at `<href>.md` for agents. `section` is the hub it belongs to, shown as
 * the eyebrow and in the breadcrumb; `cta` ends the page with the landing
 * page's closing call to action. */
export function MarkdownPage({
  href,
  eyebrow,
  section,
  cta,
}: {
  href: string;
  eyebrow: string;
  section?: { name: string; href: string };
  cta?: boolean;
}) {
  const page = docsContent(href);
  return (
    <>
      <article className="mx-auto w-full max-w-2xl px-4 pt-32 pb-24 sm:px-6 sm:pt-40">
        {section && (
          <JsonLd
            data={contentPageData({
              href,
              title: page.title,
              description: page.description,
              section,
            })}
          />
        )}
        {section ? (
          <Link href={section.href} className="text-sm font-medium text-ds-accent hover:underline">
            {eyebrow}
          </Link>
        ) : (
          <p className="text-sm font-medium text-ds-accent">{eyebrow}</p>
        )}
        <h1 className="mt-3 text-[30px] leading-[1.15] font-medium tracking-[-0.025em] text-balance sm:text-[36px]">
          {page.title}
        </h1>
        <p className="mt-3 text-base leading-relaxed text-pretty text-ds-muted">{page.description}</p>
        <div className="docs-prose mt-10">
          <DocsMarkdown>{page.body}</DocsMarkdown>
        </div>
      </article>
      {cta && <FinalCta />}
    </>
  );
}
