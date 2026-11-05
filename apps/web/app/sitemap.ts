import type { MetadataRoute } from 'next';
import { site } from '@/lib/site';
import { CONTENT_PATHS } from '@/lib/content-pages';
import { DOCS_PATHS } from '@/lib/docs';

export const dynamic = 'force-static';

/** Every page on the site. Add new routes here; docs pages come from lib/docs.ts. */
const PAGES = ['', '/pricing', '/download','/changelog', '/privacy', '/terms', ...DOCS_PATHS, ...CONTENT_PATHS];

export default function sitemap(): MetadataRoute.Sitemap {
  return PAGES.map((path) => ({ url: `${site.url}${path}`, priority: path === '' ? 1 : 0.7 }));
}
