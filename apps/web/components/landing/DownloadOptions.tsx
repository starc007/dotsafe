import { Check, Download } from 'lucide-react';
import { PLATFORMS } from '@/lib/downloads';
import { links } from '@/lib/site';
import { ButtonLink } from '@/components/landing/ButtonLink';
import { Reveal } from '@/components/landing/Reveal';
import { BrandMark } from '@/components/BrandMark';

/** One card per platform: its installers, what they include, and the standalone CLI. */
export function DownloadOptions() {
  return (
    <div className="grid gap-4 md:grid-cols-2">
      {PLATFORMS.map((p, i) => (
        <Reveal key={p.id} delay={i * 0.06}>
          <section id={p.id} className="flex h-full scroll-mt-28 flex-col rounded-2xl border border-ds-line p-6 sm:p-8">
            <div className="flex items-center gap-3">
              <span className="flex size-10 shrink-0 items-center justify-center rounded-full bg-ds-shell text-ds-fg/85">
                <BrandMark name={p.logo} className="size-5" />
              </span>
              <div className="min-w-0">
                <h2 className="text-xl leading-tight font-medium tracking-[-0.01em]">{p.name}</h2>
                <p className="text-sm text-ds-muted">{p.detail}</p>
              </div>
            </div>
            <ul className="mt-6 flex flex-col gap-2.5 text-sm">
              {p.notes.map((note) => (
                <li key={note} className="flex gap-2.5">
                  <Check aria-hidden className="mt-0.5 size-4 shrink-0 text-ds-accent" />
                  {note}
                </li>
              ))}
            </ul>
            <div className="mt-auto flex flex-wrap gap-2.5 pt-8">
              {p.downloads.map((d, j) => (
                <ButtonLink key={d.href} href={d.href} variant={j === 0 ? 'primary' : 'outline'}>
                  {d.label}
                  {j === 0 && <Download aria-hidden />}
                </ButtonLink>
              ))}
            </div>
            {p.other && (
              <p className="mt-4 text-sm text-ds-muted">
                On {p.other.arch}:{' '}
                {p.other.downloads.map((d, j) => (
                  <span key={d.href}>
                    {j > 0 && ' · '}
                    <a href={d.href} className="text-ds-fg underline decoration-ds-line underline-offset-2">
                      {d.label}
                    </a>
                  </span>
                ))}
              </p>
            )}
            <p className={`${p.other ? 'mt-2' : 'mt-4'} text-xs text-ds-muted`}>
              CLI only?{' '}
              <a href={`${links.releases}/latest`} className="underline decoration-ds-line underline-offset-2 hover:text-ds-fg">
                <code className="font-mono">{p.cli}</code>
              </a>
            </p>
          </section>
        </Reveal>
      ))}
    </div>
  );
}
