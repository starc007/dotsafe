import { FaqItem } from '@/components/landing/FaqItem';
import { Reveal } from '@/components/landing/Reveal';
import { SectionHeading } from '@/components/landing/SectionHeading';
import { FAQ } from '@/lib/faq';

/** Common questions, each opening in place. */
export function Faq() {
  return (
    <section id="faq" className="mx-auto max-w-6xl scroll-mt-20 px-4 py-20 sm:px-6 sm:py-28">
      <div className="grid gap-10 md:grid-cols-5">
        <div className="md:col-span-2">
          <SectionHeading eyebrow="FAQ" title="Questions, answered.">
            Anything else, ask on GitHub.
          </SectionHeading>
        </div>
        <Reveal className="md:col-span-3">
          <ul className="border-t border-ds-line">
            {FAQ.map(({ q, a }) => (
              <FaqItem key={q} q={q} a={a} />
            ))}
          </ul>
        </Reveal>
      </div>
    </section>
  );
}
