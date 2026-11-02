import Link from 'next/link';
import { links } from '@/lib/site';
import { ButtonLink } from '@/components/landing/ButtonLink';

export function Nav() {
  return (
    <header className="fixed inset-x-0 top-0 z-40 border-b border-ds-line/0 bg-white/60 backdrop-blur-md">
      <nav className="mx-auto flex h-16 max-w-6xl items-center gap-8 px-4 sm:px-6">
        <Link href="/" className="text-[15px] font-medium tracking-tight">
          dotsafe
        </Link>
        <div className="hidden items-center gap-6 text-sm text-ds-muted sm:flex">
          <Link href="/#features" className="transition-colors hover:text-ds-fg">
            Features
          </Link>
          <Link href="/#security" className="transition-colors hover:text-ds-fg">
            Security
          </Link>
          <Link href="/pricing" className="transition-colors hover:text-ds-fg">
            Pricing
          </Link>
          <Link href={links.docs} className="transition-colors hover:text-ds-fg">
            Docs
          </Link>
        </div>
        <div className="ml-auto flex items-center gap-2">
          <ButtonLink href={links.github} variant="ghost" className="hidden h-9 px-3 sm:inline-flex">
            GitHub
          </ButtonLink>
          <ButtonLink href={links.download} variant="primary" className="h-9 px-4">
            Download
          </ButtonLink>
        </div>
      </nav>
    </header>
  );
}
