import type { Metadata } from 'next';
import { ThanksContent } from '@/components/landing/ThanksContent';

// Where Dodo sends buyers after checkout. Kept out of search, and never sends
// a referrer, since the URL arrives carrying the license key.
export const metadata: Metadata = {
  title: 'Thanks',
  robots: { index: false, follow: false },
  referrer: 'no-referrer',
};

export default function ThanksPage() {
  return (
    <section className="mx-auto w-full max-w-6xl px-4 pt-32 pb-24 sm:px-6 sm:pt-40">
      <ThanksContent />
    </section>
  );
}
