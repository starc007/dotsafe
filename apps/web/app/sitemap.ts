import type { MetadataRoute } from 'next';
import { site } from '@/lib/site';

/** Every page on the site. Add new routes here. */
const PAGES = ['', '/pricing', '/docs', '/changelog'];

export default function sitemap(): MetadataRoute.Sitemap {
  return PAGES.map((path) => ({ url: `${site.url}${path}`, priority: path === '' ? 1 : 0.7 }));
}
