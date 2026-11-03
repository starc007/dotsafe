import { Check } from 'lucide-react';
import { PLAN } from '@/lib/pricing';
import { BuyButton } from '@/components/landing/BuyButton';
import { Reveal } from '@/components/landing/Reveal';
import { Wallpaper } from '@/components/landing/Wallpaper';

/** The one plan: its price on a painted wallpaper, everything it includes beside it. */
export function PricingPlans() {
  return (
    <Reveal className="grid gap-2 rounded-3xl border border-ds-line p-2 md:grid-cols-2">
      <Wallpaper scene="grove" className="p-6 sm:p-8">
        {/* Darkens the paint behind the copy so white text holds where it turns pale. */}
        <div aria-hidden className="absolute inset-0 bg-[linear-gradient(110deg,rgb(26_46_5/0.65),rgb(26_46_5/0.15))]" />
        <div className="relative flex h-full flex-col">
          <p className="text-sm font-medium text-white">dotsafe</p>
          <p className="mt-4 flex items-baseline gap-2.5">
            <span className="text-5xl font-medium tracking-[-0.03em] text-white tabular-nums sm:text-6xl">{PLAN.price}</span>
            <s className="text-xl tracking-[-0.01em] text-white/55 tabular-nums decoration-white/55 decoration-1">
              <span className="sr-only">instead of </span>
              {PLAN.was}
            </s>
            <span className="text-sm text-white/75">{PLAN.term}</span>
          </p>
          <p className="mt-3 max-w-xs text-sm text-pretty text-white/80">{PLAN.blurb}</p>
          <div className="mt-auto flex flex-col gap-2 pt-8">
            <BuyButton label={`Buy dotsafe for ${PLAN.price}`} className="w-full" />
            <p className="mt-1 text-center text-xs text-white/75">Your license key arrives by email.</p>
          </div>
        </div>
      </Wallpaper>
      <ul className="flex flex-col gap-3 p-6 text-sm sm:p-8">
        {PLAN.perks.map((perk) => (
          <li key={perk} className="flex gap-2.5">
            <Check aria-hidden className="mt-0.5 size-4 shrink-0 text-ds-accent" />
            {perk}
          </li>
        ))}
      </ul>
    </Reveal>
  );
}
