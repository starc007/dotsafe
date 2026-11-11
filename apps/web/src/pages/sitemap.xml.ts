import { site } from '@/lib/site';
import { CONTENT_PATHS } from '@/lib/content-pages';
import { DOCS_PATHS } from '@/lib/docs';
import { pageUpdated } from '@/lib/updated';

/** Every page on the site. Add new routes here; docs pages come from lib/docs.ts. */
const PAGES = ['', '/pricing', '/download', '/changelog', '/privacy', '/terms', ...DOCS_PATHS, ...CONTENT_PATHS];

export function GET() {
  const urls = PAGES.map((path) => {
    // Pages written as Markdown in public/ have a date; the rest have none.
    const updated = path && pageUpdated(path);
    const lastmod = updated ? `\n<lastmod>${updated}</lastmod>` : '';
    return `<url>\n<loc>${site.url}${path}</loc>${lastmod}\n<priority>${path === '' ? 1 : 0.7}</priority>\n</url>`;
  });
  return new Response(
    `<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n${urls.join('\n')}\n</urlset>\n`,
    { headers: { 'Content-Type': 'application/xml' } },
  );
}
