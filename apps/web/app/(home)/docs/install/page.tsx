import Link from 'next/link';
import { links, site } from '@/lib/site';
import { docsMetadata } from '@/lib/seo';
import { CodeBlock } from '@/components/docs/CodeBlock';
import { DocsArticle } from '@/components/docs/DocsArticle';

export const metadata = docsMetadata('/docs/install');

export default function InstallPage() {
  return (
    <DocsArticle href="/docs/install">
      <p>
        dotsafe runs on macOS, on Apple silicon and Intel. Both downloads are on the{' '}
        <a href={links.download}>latest release</a>.
      </p>

      <h2>Desktop app</h2>
      <ol>
        <li>
          Download <code>dotsafe-&lt;version&gt;.dmg</code>. It&apos;s a universal build, signed and notarized.
        </li>
        <li>Open it and drag dotsafe into Applications.</li>
        <li>Open dotsafe. The first screen asks for your license key.</li>
      </ol>
      <p>The app checks for updates on its own and installs one only when you click.</p>

      <h2>CLI</h2>
      <p>
        Download <code>dotsafe-&lt;version&gt;-macos-universal.tar.gz</code>, unpack it and put <code>dotsafe</code>{' '}
        somewhere on your <code>PATH</code>:
      </p>
      <CodeBlock>{`
tar -xzf dotsafe-*-macos-universal.tar.gz
sudo mv dotsafe /usr/local/bin/
dotsafe --version
`}</CodeBlock>

      <h2>Activate your license</h2>
      <p>
        dotsafe is paid, with no free version: one license is ${site.price} and covers up to 2 Macs. See{' '}
        <Link href="/pricing">pricing</Link>. Activate it once, in the app or the CLI. Both read the same license, so
        either is enough.
      </p>
      <CodeBlock>{`
dotsafe license activate <key>
dotsafe license status
dotsafe license remove     # free this Mac's activation to use the key elsewhere
`}</CodeBlock>
      <p>
        Being offline never locks you out. The license is rechecked about twice a day, and after 30 days without a check
        it only shows as unverified.
      </p>
    </DocsArticle>
  );
}
