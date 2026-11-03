import type { Metadata } from 'next';
import { site } from '@/lib/site';
import { docsContent } from '@/lib/docs-content';

/** The `<link rel="alternate">` types a page offers: its Markdown copy for
 * agents and, where it fits, the changelog feed. */
export function alternateTypes(markdown?: string, feed?: boolean) {
  return {
    ...(markdown && { 'text/markdown': markdown }),
    ...(feed && { 'application/rss+xml': '/changelog.xml' }),
  };
}

/** A page's metadata: its title (the layout adds "· dotsafe"), description,
 * canonical URL, Markdown copy, and matching share card text. Next replaces
 * the layout's `openGraph` and `twitter` wholesale, so each page sets them in full. */
export function pageMetadata({
  title,
  description,
  path,
  markdown,
  feed,
}: {
  title: string;
  description: string;
  path: string;
  /** Where the page's Markdown copy is, e.g. `/pricing.md`. */
  markdown?: string;
  feed?: boolean;
}): Metadata {
  const full = `${title} · ${site.name}`;
  return {
    title,
    description,
    alternates: { canonical: path, types: alternateTypes(markdown, feed) },
    // The share image from app/opengraph-image.tsx; a page's own openGraph drops it otherwise.
    openGraph: { siteName: site.name, type: 'website', url: path, title: full, description, images: '/opengraph-image' },
    twitter: { card: 'summary_large_image', title: full, description, images: '/opengraph-image' },
  };
}

/** A docs page's metadata, from the title and description in its Markdown. */
export function docsMetadata(href: string): Metadata {
  const { title, description } = docsContent(href);
  return pageMetadata({
    title: href === '/docs' ? 'Docs' : `${title} · Docs`,
    description,
    path: href,
    markdown: `${href}.md`,
  });
}
