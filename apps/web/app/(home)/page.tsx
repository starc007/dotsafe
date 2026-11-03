import { Features } from '@/components/landing/Features';
import { FinalCta } from '@/components/landing/FinalCta';
import { Hero } from '@/components/landing/Hero';
import { Faq } from '@/components/landing/Faq';
import { JsonLd } from '@/components/landing/JsonLd';
import { Security } from '@/components/landing/Security';
import { StorageStrip } from '@/components/landing/StorageStrip';
import { alternateTypes } from '@/lib/seo';
import { faqData, homeData } from '@/lib/structured-data';

// The layout's title and share card; this adds the Markdown copy and the feed.
export const metadata = { alternates: { canonical: '/', types: alternateTypes('/index.md', true) } };

export default function HomePage() {
  return (
    <>
      <JsonLd data={homeData} />
      <JsonLd data={faqData} />
      <Hero />
      <StorageStrip />
      <Features />
      <Security />
      <Faq />
      <FinalCta />
    </>
  );
}
