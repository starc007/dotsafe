import type { Metadata } from 'next';
import { site } from '@/lib/site';

/** A page's metadata: its title (the layout adds "· dotsafe"), description,
 * canonical URL, and matching share card text. Next replaces the layout's
 * `openGraph` and `twitter` wholesale, so each page sets them in full. */
export function pageMetadata({ title, description, path }: { title: string; description: string; path: string }): Metadata {
  const full = `${title} · ${site.name}`;
  return {
    title,
    description,
    alternates: { canonical: path },
    // The share image from app/opengraph-image.tsx; a page's own openGraph drops it otherwise.
    openGraph: { siteName: site.name, type: 'website', url: path, title: full, description, images: '/opengraph-image' },
    twitter: { card: 'summary_large_image', title: full, description, images: '/opengraph-image' },
  };
}
