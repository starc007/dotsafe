import type { Metadata } from 'next';
import { Faq } from '@/components/landing/Faq';
import { FinalCta } from '@/components/landing/FinalCta';
import { PricingPlans } from '@/components/landing/PricingPlans';

export const metadata: Metadata = {
  title: 'Pricing · dotsafe',
  description: 'One license, $49 once, for up to 2 Macs.',
};

export default function PricingPage() {
  return (
    <>
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
