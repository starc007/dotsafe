import Link from 'next/link';
import { DownloadOptions } from '@/components/landing/DownloadOptions';
import { pageMetadata } from '@/lib/seo';
import { links } from '@/lib/site';

export const metadata = pageMetadata({
  title: 'Download',
  description: 'Download dotsafe for macOS (Apple silicon and Intel) or Linux (.deb and AppImage). The app and the CLI share one encrypted vault.',
  path: '/download',
  markdown: '/download.md',
});

export default function DownloadPage() {
  return (
    <section className="mx-auto max-w-6xl px-4 pt-32 pb-24 sm:px-6 sm:pt-40">
      <h1 className="max-w-2xl text-[28px] leading-[1.2] font-medium tracking-[-0.02em] text-balance sm:text-[36px]">
        Download dotsafe. <span className="text-ds-muted">For macOS and Linux.</span>
      </h1>
      <div className="mt-12">
        <DownloadOptions />
      </div>
      <p className="mt-8 text-sm text-ds-muted">
        The first screen asks for your license key.{' '}
        <Link href="/pricing" className="text-ds-fg underline decoration-ds-line underline-offset-2">
          Get a license
        </Link>
        , read the{' '}
        <Link href="/docs/install" className="text-ds-fg underline decoration-ds-line underline-offset-2">
          install guide
        </Link>
        , or see{' '}
        <a href={links.releases} className="text-ds-fg underline decoration-ds-line underline-offset-2">
          every release
        </a>
        .
      </p>
    </section>
  );
}
