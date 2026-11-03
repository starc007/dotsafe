import Link from 'next/link';
import { docsMetadata } from '@/lib/seo';
import { DocsArticle } from '@/components/docs/DocsArticle';

export const metadata = docsMetadata('/docs');

export default function DocsPage() {
  return (
    <DocsArticle href="/docs">
      <p>
        dotsafe keeps your environment variables and API keys encrypted on your Mac and syncs them through storage you
        already own: iCloud Drive, Google Drive, Dropbox, an S3 or R2 bucket, or WebDAV. There&apos;s no dotsafe server
        holding your vault.
      </p>
      <p>
        It comes as a desktop app and a <code>dotsafe</code> command-line tool. Both open the same vault, so a change in
        one shows up in the other.
      </p>

      <h2>How it works</h2>
      <ul>
        <li>
          <strong>Encrypted on your device.</strong> Your master password never leaves your Mac. The vault is a folder
          of encrypted files, and whatever syncs it only ever sees ciphertext.
        </li>
        <li>
          <strong>Synced by storage you own.</strong> Put the vault in a synced folder or a bucket, and every Mac that
          opens it stays in step. Each one keeps a copy, so dotsafe works offline.
        </li>
        <li>
          <strong>Injected, not written down.</strong> <code>dotsafe run</code> starts your app with its variables set,
          so there&apos;s no <code>.env</code> file to leak or commit.
        </li>
        <li>
          <strong>Safe for agents.</strong> Claude Code, Cursor and other MCP agents can run commands with your secrets
          after you approve, without ever seeing a value.
        </li>
      </ul>

      <h2>Where to start</h2>
      <ul>
        <li>
          <Link href="/docs/install">Install</Link> the app and the CLI.
        </li>
        <li>
          Follow the <Link href="/docs/quick-start">quick start</Link> to move a project&apos;s <code>.env</code> into a
          vault.
        </li>
        <li>
          Read <Link href="/docs/security">Security</Link> for what dotsafe protects against, and what it doesn&apos;t.
        </li>
      </ul>
    </DocsArticle>
  );
}
