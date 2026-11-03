import Link from 'next/link';
import { docsMetadata } from '@/lib/seo';
import { CodeBlock } from '@/components/docs/CodeBlock';
import { DocsArticle } from '@/components/docs/DocsArticle';

export const metadata = docsMetadata('/docs/quick-start');

export default function QuickStartPage() {
  return (
    <DocsArticle href="/docs/quick-start">
      <p>
        This takes a project with a <code>.env</code> file and moves it into a vault. You can do the same in the app;
        here it&apos;s the CLI.
      </p>

      <h2>1. Create a vault</h2>
      <CodeBlock>{`dotsafe init`}</CodeBlock>
      <p>
        It asks for a master password and puts the vault in <code>iCloud Drive/dotsafe</code>. Pass{' '}
        <code>--path</code> for another synced folder, or see <Link href="/docs/storage">Storage and sync</Link> for
        buckets and WebDAV. If a vault is already there, <code>init</code> connects this Mac to it instead.
      </p>
      <p>
        Write down the recovery key it prints. It&apos;s the only way back in if you forget the password.
      </p>

      <h2>2. Import your .env</h2>
      <CodeBlock>{`
cd ~/code/my-app
dotsafe import             # reads ./.env into the development environment
dotsafe list               # names only
`}</CodeBlock>
      <p>
        The first write in a directory creates a project named after it, and a <code>.dotsafe.json</code> file that
        links the two. It only holds ids, so commit it: other Macs on the same vault pick the project up.
      </p>

      <h2>3. Run your app</h2>
      <CodeBlock>{`dotsafe run -- bun dev`}</CodeBlock>
      <p>
        The command gets every variable in its environment. Now delete the <code>.env</code> file: it isn&apos;t needed
        anymore.
      </p>

      <h2>4. Add and change variables</h2>
      <CodeBlock>{`
dotsafe set API_KEY                # asks for the value, so it stays out of shell history
dotsafe set -e production A=1 B=2  # another environment
dotsafe get API_KEY
dotsafe unset API_KEY
`}</CodeBlock>

      <h2>5. Skip the password prompt</h2>
      <CodeBlock>{`
dotsafe unlock --for 8h    # keep the key in the macOS keychain for a while
dotsafe lock               # forget it now
`}</CodeBlock>

      <h2>Next</h2>
      <ul>
        <li>
          <Link href="/docs/environments">Projects and environments</Link>: staging, production and inheritance.
        </li>
        <li>
          <Link href="/docs/secrets">Secrets and rotation</Link>: one key used by many projects.
        </li>
        <li>
          <Link href="/docs/agents">AI agents</Link>: let your coding agent run with secrets it never sees.
        </li>
      </ul>
    </DocsArticle>
  );
}
