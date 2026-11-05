import Link from 'next/link';
import { ArrowUpRight } from 'lucide-react';
import type { ContentItem, Section } from '@/lib/content-pages';
import { FinalCta } from '@/components/landing/FinalCta';

/** A hub page: every page of a section as a card, grouped, linking each one
 * so none is an orphan. Cards show the short label and blurb from
 * lib/content-pages.ts; the full title and description are on the page. */
export function ContentHub({ section }: { section: Section }) {
  return (
    <>
      <section className="mx-auto w-full max-w-5xl px-4 pt-32 pb-24 sm:px-6 sm:pt-40">
        <h1 className="max-w-2xl text-[30px] leading-[1.15] font-medium tracking-[-0.025em] text-balance sm:text-[36px]">
          {section.title}
        </h1>
        <p className="mt-3 max-w-2xl text-base leading-relaxed text-pretty text-ds-muted">{section.description}</p>
        {section.groups.map((group) => (
          <div key={group.title} className="mt-12">
            <h2 className="text-sm font-medium text-ds-muted">{group.title}</h2>
            <ul className="mt-4 grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
              {group.items.map((item) => (
                <li key={item.slug}>
                  <HubCard item={item} href={`${section.href}/${item.slug}`} />
                </li>
              ))}
            </ul>
          </div>
        ))}
      </section>
      <FinalCta />
    </>
  );
}

function HubCard({ item, href }: { item: ContentItem; href: string }) {
  return (
    <Link
      href={href}
      className="group relative flex h-full flex-col gap-4 rounded-2xl border border-ds-line p-4 transition-colors duration-200 hover:border-ds-fg/20 focus-visible:border-ds-fg/30 focus-visible:outline-none"
    >
      <div className="flex items-center gap-3">
        <span className="flex size-9 shrink-0 items-center justify-center rounded-full bg-ds-shell text-ds-fg/85">
          {item.logo ? <Logo src={`/brands/${item.logo}.svg`} /> : <Initials label={item.label} />}
        </span>
        <span className="min-w-0 truncate text-[15px] font-medium">{item.label}</span>
        <ArrowUpRight
          aria-hidden
          className="ml-auto size-4 shrink-0 -translate-x-1 translate-y-1 text-ds-muted opacity-0 transition-[opacity,transform] duration-200 ease-[cubic-bezier(0.16,1,0.3,1)] group-hover:translate-x-0 group-hover:translate-y-0 group-hover:opacity-100 group-focus-visible:translate-x-0 group-focus-visible:translate-y-0 group-focus-visible:opacity-100 motion-reduce:transition-none"
        />
      </div>
      <p className="line-clamp-2 text-sm leading-snug text-pretty text-ds-muted">{item.blurb}</p>
    </Link>
  );
}

/** A product's mark as a one-colour mask: every logo, gradients included,
 * takes the chip's text colour, so the cards read as one set. Decorative:
 * the product's name is right beside it. */
function Logo({ src }: { src: string }) {
  const mask = `url(${src}) center / contain no-repeat`;
  return <span aria-hidden className="size-[18px] bg-current" style={{ mask, WebkitMask: mask }} />;
}

/** "HashiCorp Vault" → "HV", "1Password" → "1P": a neutral mark instead of a
 * brand logo. */
function Initials({ label }: { label: string }) {
  const words = label.split(/\s+/);
  const text = words.length > 1 ? words[0][0] + words[1][0] : (label.match(/^\d?[a-zA-Z]/)?.[0] ?? label[0]);
  return <span className="text-xs font-semibold tracking-tight uppercase">{text}</span>;
}
