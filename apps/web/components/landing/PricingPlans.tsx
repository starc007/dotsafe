import { Check, Download } from 'lucide-react';
import { links } from '@/lib/site';
import { cn } from '@/lib/cn';
import { ButtonLink } from '@/components/landing/ButtonLink';
import { Reveal } from '@/components/landing/Reveal';
import { Wallpaper } from '@/components/landing/Wallpaper';

// Placeholder pricing until the real plans are set.
const PLANS = [
  {
    name: 'Free',
    price: '$0',
    term: 'forever',
    blurb: 'Everything you need for your own projects on one Mac.',
    perks: ['1 Mac', 'Unlimited variables and environments', 'The dotsafe CLI', 'Any storage you already use', 'Import from .env, Doppler, Infisical and 1Password'],
    cta: 'Download',
  },
  {
    name: 'Pro',
    price: '$39',
    term: 'one-time',
    blurb: 'For keys you share across projects, rotate often, or hand to agents.',
    perks: ['Up to 3 Macs', 'Secrets linked across environments', 'Key rotation through OpenAI, Cloudflare and AWS', 'Agent access approved with Touch ID', 'A year of updates'],
    cta: 'Download and try Pro',
    featured: true,
  },
];

/** The two plans side by side. Pro's header sits on a painted wallpaper. */
export function PricingPlans() {
  return (
    <div className="grid gap-4 md:grid-cols-2">
      {PLANS.map((plan, i) => {
        const header = (
          <>
            <p className={cn('text-sm font-medium', plan.featured ? 'text-white' : 'text-ds-fg')}>{plan.name}</p>
            <p className="mt-4 flex items-baseline gap-1.5">
              <span className={cn('text-4xl font-medium tracking-[-0.02em] tabular-nums', plan.featured && 'text-white')}>
                {plan.price}
              </span>
              <span className={cn('text-sm', plan.featured ? 'text-white/75' : 'text-ds-muted')}>{plan.term}</span>
            </p>
            <p className={cn('mt-3 max-w-xs text-sm text-pretty', plan.featured ? 'text-white/80' : 'text-ds-muted')}>
              {plan.blurb}
            </p>
            <ButtonLink href={links.download} variant={plan.featured ? 'primary' : 'outline'} className="mt-6 w-full">
              {plan.cta}
              <Download aria-hidden />
            </ButtonLink>
          </>
        );
        return (
          <Reveal key={plan.name} delay={i * 0.08} className="flex flex-col rounded-3xl border border-ds-line p-2">
            {plan.featured ? (
              <Wallpaper scene="grove" className="p-6">
                {/* Darkens the paint behind the copy so white text holds where it turns pale. */}
                <div aria-hidden className="absolute inset-0 bg-[linear-gradient(110deg,rgb(26_46_5/0.65),rgb(26_46_5/0.15))]" />
                <div className="relative">{header}</div>
              </Wallpaper>
            ) : (
              <div className="p-6">{header}</div>
            )}
            <ul className="flex flex-col gap-3 p-6 text-sm">
              {plan.perks.map((perk) => (
                <li key={perk} className="flex gap-2.5">
                  <Check aria-hidden className="mt-0.5 size-4 shrink-0 text-ds-accent" />
                  {perk}
                </li>
              ))}
            </ul>
          </Reveal>
        );
      })}
    </div>
  );
}
