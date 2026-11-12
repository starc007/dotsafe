import { docsContent } from '@/lib/docs-content';

/** What a page puts in its `<head>` (src/components/Head.astro). Without a
 * title or description it uses the site's own. */
export type PageMeta = {
  /** The page's own title; the head adds "· dotsafe". */
  title?: string;
  description?: string;
  /** The page's address, for its canonical URL. Leave out to have none. */
  path?: string;
  /** Where the page's Markdown copy is, e.g. `/pricing.md`. */
  markdown?: string;
  /** Offers the changelog feed. */
  feed?: boolean;
  /** Offers the blog's feed. */
  blogFeed?: boolean;
  /** Kept out of search, and never sends a referrer. */
  private?: boolean;
};

/** The share image (src/pages/og.png.ts). */
export const ogImage = {
  path: '/og.png',
  alt: 'dotsafe: secrets that never leave your Mac',
  width: 1200,
  height: 630,
};

/** A docs page's head, from the title and description in its Markdown. */
export function docsMeta(href: string): PageMeta {
  const { title, description } = docsContent(href);
  return {
    title: href === '/docs' ? 'Docs' : `${title} · Docs`,
    description,
    path: href,
    markdown: `${href}.md`,
  };
}
