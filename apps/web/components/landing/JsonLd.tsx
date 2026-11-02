import { links, site } from '@/lib/site';

/** Structured data for search engines: dotsafe as a paid macOS developer app. */
export function JsonLd() {
  const data = {
    '@context': 'https://schema.org',
    '@type': 'SoftwareApplication',
    name: site.name,
    description: site.description,
    url: site.url,
    applicationCategory: 'DeveloperApplication',
    operatingSystem: 'macOS 13 or later',
    downloadUrl: links.download,
    offers: { '@type': 'Offer', price: site.price, priceCurrency: 'USD' },
  };
  // Escapes `<` so nothing in the data can close the script tag.
  return (
    <script type="application/ld+json" dangerouslySetInnerHTML={{ __html: JSON.stringify(data).replace(/</g, '\\u003c') }} />
  );
}
