import Link from 'next/link';
import { docsMetadata } from '@/lib/seo';
import { CodeBlock } from '@/components/docs/CodeBlock';
import { DocsArticle } from '@/components/docs/DocsArticle';

export const metadata = docsMetadata('/docs/devices');

export default function DevicesPage() {
  return (
    <DocsArticle href="/docs/devices">
      <p>
        Every Mac that opens the vault can have its own key, kept in its keychain. That&apos;s what lets you unlock with
        Touch ID, and cut off one Mac without changing anything on the others.
      </p>

      <h2>Touch ID unlock</h2>
      <p>
        Turn it on in Settings → Devices. The vault then unlocks with Touch ID, and asks for the master password every
        two weeks so you don&apos;t forget it.
      </p>

      <h2>Adding a Mac</h2>
      <p>
        On a Mac that&apos;s already set up, create a one-time code in Settings → Devices. Enter it on the new Mac
        instead of typing the master password there. The new Mac needs access to the same storage.
      </p>

      <h2>Removing a device</h2>
      <p>If a Mac is lost, sold or compromised, remove it from any other device:</p>
      <CodeBlock>{`
dotsafe device list
dotsafe device remove "Old MacBook"
`}</CodeBlock>
      <p>
        dotsafe switches the vault to a new key and re-encrypts everything, so the removed Mac can&apos;t read anything
        written after that. It prints a new recovery key; keep it safe.
      </p>
      <p>
        A removed Mac may still have copies of values it read before. So every secret it could read goes into a
        rotation queue, shown in the sidebar until each one is rotated. Secrets with a{' '}
        <Link href="/docs/integrations">provider</Link> rotate in one step:
      </p>
      <CodeBlock>{`dotsafe secret queue --rotate`}</CodeBlock>
    </DocsArticle>
  );
}
