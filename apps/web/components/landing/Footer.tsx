import Link from 'next/link';
import { links } from '@/lib/site';

export function Footer() {
  return (
    <footer className="border-t border-ds-line">
      <div className="mx-auto flex w-full max-w-6xl flex-wrap items-center gap-x-6 gap-y-3 px-4 py-8 text-sm text-ds-muted sm:px-6">
        <span className="font-medium tracking-tight text-ds-fg">dotsafe</span>
        <Link href="/#features" className="transition-colors hover:text-ds-fg">
          Features
        </Link>
        <Link href="/#security" className="transition-colors hover:text-ds-fg">
          Security
        </Link>
        <Link href="/#pricing" className="transition-colors hover:text-ds-fg">
          Pricing
        </Link>
        <Link href={links.docs} className="transition-colors hover:text-ds-fg">
          Docs
        </Link>
        <Link href={links.github} className="transition-colors hover:text-ds-fg">
          GitHub
        </Link>
        <span className="ml-auto">© 2026 dotsafe</span>
      </div>
    </footer>
  );
}
