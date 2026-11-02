import type { Metadata } from 'next';

export const metadata: Metadata = {
  title: 'Docs · dotsafe',
  description: 'A local-first, zero-knowledge secret manager for developers.',
};

export default function DocsPage() {
  return (
    <article className="mx-auto w-full max-w-2xl px-4 pt-32 pb-24 sm:px-6 sm:pt-40">
      <p className="text-sm text-ds-muted">Docs</p>
      <h1 className="mt-3 text-[28px] leading-[1.2] font-medium tracking-[-0.02em]">Introduction</h1>
      <div className="mt-6 flex flex-col gap-4 text-[15px] leading-relaxed text-pretty text-ds-fg/80">
        <p>
          dotsafe keeps your environment variables and API keys encrypted on your Mac and syncs them through storage you
          already own: iCloud Drive, Google Drive, Dropbox, an S3 or R2 bucket, or WebDAV. There&apos;s no dotsafe
          server holding your vault.
        </p>
        <p>
          It comes as a desktop app and a <code className="font-mono text-[13px]">dotsafe</code> command-line tool that
          share the same vault.
        </p>
      </div>
    </article>
  );
}
