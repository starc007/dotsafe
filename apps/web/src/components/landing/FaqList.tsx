import { FAQ } from '@/lib/faq';
import { FaqItem } from '@/components/landing/FaqItem';

/** Every question, as one island (Faq.astro). */
export function FaqList() {
  return (
    <ul className="border-t border-ds-line">
      {FAQ.map(({ q, a }) => (
        <FaqItem key={q} q={q} a={a} />
      ))}
    </ul>
  );
}
