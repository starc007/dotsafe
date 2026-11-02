import { Features } from '@/components/landing/Features';
import { FinalCta } from '@/components/landing/FinalCta';
import { Hero } from '@/components/landing/Hero';
import { Pricing } from '@/components/landing/Pricing';
import { Security } from '@/components/landing/Security';
import { StorageStrip } from '@/components/landing/StorageStrip';

export default function HomePage() {
  return (
    <>
      <Hero />
      <StorageStrip />
      <Features />
      <Security />
      <Pricing />
      <FinalCta />
    </>
  );
}
