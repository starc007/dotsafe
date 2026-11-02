import { Features } from '@/components/landing/Features';
import { FinalCta } from '@/components/landing/FinalCta';
import { Hero } from '@/components/landing/Hero';
import { Faq } from '@/components/landing/Faq';
import { JsonLd } from '@/components/landing/JsonLd';
import { Security } from '@/components/landing/Security';
import { StorageStrip } from '@/components/landing/StorageStrip';

export default function HomePage() {
  return (
    <>
      <JsonLd />
      <Hero />
      <StorageStrip />
      <Features />
      <Security />
      <Faq />
      <FinalCta />
    </>
  );
}
