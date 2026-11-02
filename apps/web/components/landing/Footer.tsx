import Link from 'next/link';
import { Download } from 'lucide-react';
import { links } from '@/lib/site';
import { ButtonLink } from '@/components/landing/ButtonLink';
import { Logo } from '@/components/landing/Logo';

const COLUMNS = [
  {
    title: 'Product',
    items: [
      { label: 'Features', href: '/#features' },
      { label: 'Security', href: '/#security' },
      { label: 'Pricing', href: '/pricing' },
      { label: 'FAQ', href: '/#faq' },
    ],
  },
  {
    title: 'Resources',
    items: [
      { label: 'Docs', href: links.docs },
      { label: 'Changelog', href: '/changelog' },
      { label: 'Releases', href: links.releases },
      { label: 'GitHub', href: links.github },
      { label: 'Report an issue', href: links.issues },
    ],
  },
];

/** Brand and download on the left, link columns on the right, and the
 * wordmark set large underneath, fading into the page. */
export function Footer() {
  return (
    <footer className="overflow-hidden border-t border-ds-line">
      <div className="mx-auto grid max-w-6xl gap-12 px-4 pt-16 sm:px-6 md:grid-cols-[1fr_auto]">
        <div className="max-w-xs">
          <Logo />
          <p className="mt-2 text-sm text-pretty text-ds-muted">
            Secrets encrypted on your Mac, synced through storage you already own.
          </p>
          <ButtonLink href={links.download} variant="primary" className="mt-6 h-9 px-4">
            Download for macOS
            <Download aria-hidden />
          </ButtonLink>
        </div>
        <nav aria-label="Footer" className="grid grid-cols-2 gap-x-16 gap-y-10">
          {COLUMNS.map((col) => (
            <div key={col.title}>
              <p className="text-sm font-medium">{col.title}</p>
              <ul className="mt-4 flex flex-col gap-2.5 text-sm text-ds-muted">
                {col.items.map((item) => (
                  <li key={item.label}>
                    <Link href={item.href} className="transition-colors hover:text-ds-fg">
                      {item.label}
                    </Link>
                  </li>
                ))}
              </ul>
            </div>
          ))}
        </nav>
      </div>
      <div className="mx-auto flex max-w-6xl flex-wrap items-center justify-between gap-2 px-4 pt-14 text-xs text-ds-muted sm:px-6">
        <span>© 2026 dotsafe</span>
        <span>macOS 13 or later · Open source</span>
      </div>
      <p
        aria-hidden
        className="mx-auto -mb-[0.22em] max-w-6xl bg-gradient-to-b from-ds-fg/[0.09] to-ds-fg/0 bg-clip-text px-4 pt-6 text-center text-[clamp(5rem,22vw,17rem)] leading-none font-medium tracking-[-0.06em] text-transparent select-none sm:px-6"
      >
        dotsafe
      </p>
    </footer>
  );
}
