import { FAQ } from '@/lib/faq';
import { demo, links, site } from '@/lib/site';

/** schema.org data for <JsonLd>, so search engines and agents read the
 * product, its price, the FAQ and the docs without parsing the layout. */

const publisher = { '@type': 'Organization', name: site.name, url: site.url, sameAs: [links.github] };

/** The home page: the site and dotsafe as a paid macOS developer app. */
export const homeData = {
  '@context': 'https://schema.org',
  '@graph': [
    { '@type': 'WebSite', name: site.name, url: site.url, description: site.description, publisher },
    {
      '@type': 'SoftwareApplication',
      name: site.name,
      description: site.description,
      url: site.url,
      applicationCategory: 'DeveloperApplication',
      operatingSystem: 'macOS 13 or later, Linux (x86_64 and arm64)',
      downloadUrl: links.download,
      license: 'https://opensource.org/license/mit',
      offers: { '@type': 'Offer', price: site.price, priceCurrency: 'USD', url: `${site.url}/pricing` },
      publisher,
    },
    {
      '@type': 'VideoObject',
      name: 'dotsafe demo',
      description:
        'A one-minute walkthrough of dotsafe: add a variable, switch environments, find any key and lock the vault.',
      thumbnailUrl: `${site.url}${demo.poster}`,
      contentUrl: `${site.url}${demo.mp4}`,
      uploadDate: demo.uploaded,
      duration: demo.duration,
      width: demo.width,
      height: demo.height,
      publisher,
    },
  ],
};

/** The FAQ shown on the home and pricing pages. */
export const faqData = {
  '@context': 'https://schema.org',
  '@type': 'FAQPage',
  mainEntity: FAQ.map(({ q, a }) => ({ '@type': 'Question', name: q, acceptedAnswer: { '@type': 'Answer', text: a } })),
};

/** A docs page, and where it sits under Docs. */
export function docsData({
  href,
  title,
  description,
  updated,
}: {
  href: string;
  title: string;
  description: string;
  /** `YYYY-MM-DD`, from lib/updated.ts. */
  updated?: string;
}) {
  const url = `${site.url}${href}`;
  const crumbs = [{ name: 'Docs', url: `${site.url}/docs` }, ...(href === '/docs' ? [] : [{ name: title, url }])];
  return {
    '@context': 'https://schema.org',
    '@graph': [
      {
        '@type': 'TechArticle',
        headline: title,
        description,
        url,
        ...(updated && { dateModified: updated }),
        encoding: { '@type': 'MediaObject', encodingFormat: 'text/markdown', contentUrl: `${url}.md` },
        about: { '@type': 'SoftwareApplication', name: site.name, url: site.url },
        publisher,
      },
      {
        '@type': 'BreadcrumbList',
        itemListElement: crumbs.map((c, i) => ({ '@type': 'ListItem', position: i + 1, name: c.name, item: c.url })),
      },
    ],
  };
}

/** An alternatives or use-case page, and where it sits under its hub. */
export function contentPageData({
  href,
  title,
  description,
  section,
  updated,
}: {
  href: string;
  title: string;
  description: string;
  section?: { name: string; href: string };
  /** `YYYY-MM-DD`, from lib/updated.ts. */
  updated?: string;
}) {
  const url = `${site.url}${href}`;
  const crumbs = [
    ...(section ? [{ name: section.name, url: `${site.url}${section.href}` }] : []),
    { name: title, url },
  ];
  return {
    '@context': 'https://schema.org',
    '@graph': [
      {
        '@type': 'WebPage',
        name: title,
        description,
        url,
        ...(updated && { dateModified: updated }),
        encoding: { '@type': 'MediaObject', encodingFormat: 'text/markdown', contentUrl: `${url}.md` },
        about: { '@type': 'SoftwareApplication', name: site.name, url: site.url },
        publisher,
      },
      {
        '@type': 'BreadcrumbList',
        itemListElement: crumbs.map((c, i) => ({ '@type': 'ListItem', position: i + 1, name: c.name, item: c.url })),
      },
    ],
  };
}
