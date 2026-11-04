import { Faq } from '@/components/landing/Faq';
import { FinalCta } from '@/components/landing/FinalCta';
import { PricingPlans } from '@/components/landing/PricingPlans';
import { JsonLd } from '@/components/landing/JsonLd';
import { pageMetadata } from '@/lib/seo';
import { faqData } from '@/lib/structured-data';

export const metadata = pageMetadata({
  title: 'Pricing',
  description: 'One dotsafe license, $49 once, for up to 2 Macs. Every feature included, with lifetime updates.',
  path: '/pricing',
  markdown: '/pricing.md',
});

export default function PricingPage() {
  return (
    <>
      <JsonLd data={faqData} />
      <section className="mx-auto max-w-6xl px-4 pt-32 sm:px-6 sm:pt-40">
        <h1 className="max-w-2xl text-[28px] leading-[1.2] font-medium tracking-[-0.02em] text-balance sm:text-[36px]">
          Pay once, keep it. <span className="text-ds-muted">One license for everything.</span>
        </h1>
        <div className="mt-12">
          <PricingPlans />
        </div>
      </section>
      <Faq />
      <FinalCta />
    </>
  );
}
