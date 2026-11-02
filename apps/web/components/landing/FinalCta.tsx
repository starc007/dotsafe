import { Download } from 'lucide-react';
import { links } from '@/lib/site';
import { ButtonLink } from '@/components/landing/ButtonLink';
import { Reveal } from '@/components/landing/Reveal';
import { Wallpaper } from '@/components/landing/Wallpaper';

export function FinalCta() {
  return (
    <section className="mx-auto max-w-6xl px-4 pb-24 sm:px-6">
      <Reveal>
        <Wallpaper scene="grove" className="flex flex-col items-center px-6 py-20 text-center sm:py-28">
          {/* A soft dark pool behind the copy, so white text holds wherever the paint turns pale. */}
          <div aria-hidden className="absolute inset-0 bg-[radial-gradient(ellipse_50%_60%_at_center,rgb(26_46_5/0.7),transparent)]" />
          <h2 className="relative max-w-md text-[26px] leading-[1.2] font-medium tracking-[-0.015em] text-balance text-white sm:text-[32px]">
            Keep your secrets yours.
          </h2>
          <p className="relative mt-3 max-w-sm text-[15px] text-pretty text-white/80">
            Set up a vault in a minute and bring your .env files with you.
          </p>
          <div className="relative mt-8 flex flex-wrap justify-center gap-2.5">
            <ButtonLink href={links.download} variant="primary">
              Download for macOS
              <Download aria-hidden />
            </ButtonLink>
            <ButtonLink href={links.docs}>Read the docs</ButtonLink>
          </div>
        </Wallpaper>
      </Reveal>
    </section>
  );
}
