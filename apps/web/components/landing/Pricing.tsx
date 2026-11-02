import { Check } from 'lucide-react';
import { links } from '@/lib/site';
import { ButtonLink } from '@/components/landing/ButtonLink';
import { Reveal } from '@/components/landing/Reveal';
import { SectionHeading } from '@/components/landing/SectionHeading';

// Placeholder pricing until the real plans are set.
const PLANS = [
  {
    name: 'Free',
    price: '$0',
    term: 'forever',
    perks: ['1 Mac', 'Unlimited variables', 'The CLI', 'Any storage'],
    cta: 'Download',
  },
  {
    name: 'Pro',
    price: '$39',
    term: 'one-time',
    perks: ['Up to 3 Macs', 'Secrets and rotation', 'Agents with Touch ID', 'A year of updates'],
    cta: 'Download and try Pro',
    primary: true,
  },
];

export function Pricing() {
  return (
    <section id="pricing" className="mx-auto max-w-6xl scroll-mt-20 px-4 py-20 sm:px-6 sm:py-28">
      <SectionHeading eyebrow="Pricing" title="Pay once, keep it.">
        Start free on one Mac. Upgrade when your vault outgrows it.
      </SectionHeading>
      <div className="mt-10 grid max-w-3xl gap-4 sm:grid-cols-2">
        {PLANS.map((plan, i) => (
          <Reveal
            key={plan.name}
            delay={i * 0.08}
            className={`flex flex-col rounded-2xl border p-6 ${plan.primary ? 'border-ds-accent/40' : 'border-ds-line'}`}
          >
            <p className="text-sm font-medium">{plan.name}</p>
            <p className="mt-3 flex items-baseline gap-1.5">
              <span className="text-2xl font-medium tabular-nums">{plan.price}</span>
              <span className="text-sm text-ds-muted">{plan.term}</span>
            </p>
            <ul className="mt-6 flex flex-1 flex-col gap-2.5 text-sm">
              {plan.perks.map((perk) => (
                <li key={perk} className="flex items-center gap-2.5">
                  <Check aria-hidden className="size-4 text-ds-accent" />
                  {perk}
                </li>
              ))}
            </ul>
            <ButtonLink href={links.download} variant={plan.primary ? 'primary' : 'outline'} className="mt-8">
              {plan.cta}
            </ButtonLink>
          </Reveal>
        ))}
      </div>
    </section>
  );
}
