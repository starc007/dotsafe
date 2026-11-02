import { Download } from 'lucide-react';
import { links } from '@/lib/site';
import { ButtonLink } from '@/components/landing/ButtonLink';
import { Reveal } from '@/components/landing/Reveal';
import { ShaderBackdrop } from '@/components/landing/ShaderBackdrop';

export function FinalCta() {
  return (
    <section className="mx-auto max-w-6xl px-4 pb-24 sm:px-6">
      <Reveal className="relative overflow-hidden rounded-3xl border border-ds-line">
        <ShaderBackdrop speed={0.2} />
        <div className="absolute inset-0 bg-[linear-gradient(90deg,#fff_30%,rgb(255_255_255/0.5)_60%,rgb(255_255_255/0)_90%)]" />
        <div className="relative px-6 py-14 sm:px-10 sm:py-20">
          <h2 className="max-w-md text-[22px] leading-[1.3] font-medium text-balance sm:text-2xl">
            Keep your secrets yours.{' '}
            <span className="text-ds-muted">Set up a vault in a minute and bring your .env files with you.</span>
          </h2>
          <div className="mt-7 flex flex-wrap gap-2.5">
            <ButtonLink href={links.download} variant="primary">
              <Download aria-hidden />
              Download for Mac
            </ButtonLink>
            <ButtonLink href={links.docs}>Read the docs</ButtonLink>
          </div>
        </div>
      </Reveal>
    </section>
  );
}
