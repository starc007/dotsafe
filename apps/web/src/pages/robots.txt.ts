import { site } from '@/lib/site';

export function GET() {
  return new Response(`User-Agent: *\nAllow: /\n\nSitemap: ${site.url}/sitemap.xml\n`);
}
