import { docsMetadata } from '@/lib/seo';
import { CodeBlock } from '@/components/docs/CodeBlock';
import { DocsArticle } from '@/components/docs/DocsArticle';

export const metadata = docsMetadata('/docs/import-export');

export default function ImportExportPage() {
  return (
    <DocsArticle href="/docs/import-export">
      <h2>Import from a file</h2>
      <CodeBlock>{`
dotsafe import                        # ./.env into the linked environment
dotsafe import .env.production -e production
dotsafe import secrets.json           # JSON exported by Doppler, Infisical or 1Password
`}</CodeBlock>
      <p>
        In the app, drop <code>.env</code> files on the window, or use Import. You see what will change before anything
        is saved.
      </p>

      <h2>Import from another secret manager</h2>
      <p>dotsafe can read straight from another tool&apos;s CLI, if it&apos;s installed and signed in:</p>
      <CodeBlock>{`
dotsafe import --from doppler             # the project and config set up in this directory
dotsafe import --from infisical           # the project linked in this directory
dotsafe import --from 1password --item "my-api prod"   # one item's fields, by label
`}</CodeBlock>

      <h2>Export</h2>
      <p>
        Prefer <code>dotsafe run</code>, which never writes values to disk. For tools that only read a file:
      </p>
      <CodeBlock>{`
dotsafe export                 # print .env lines
dotsafe export -o .env         # write a file
`}</CodeBlock>
      <p>
        A written file is readable by you only. dotsafe refuses to write it if git would commit it; add it to{' '}
        <code>.gitignore</code> first, or pass <code>--force</code>.
      </p>

      <h2>Keep values out of git</h2>
      <p>
        <code>dotsafe scan</code> looks through files for any value in your vault, as is or encoded (base64, hex, URL
        encoding). It reports names and places, never the values.
      </p>
      <CodeBlock>{`
dotsafe scan                   # this directory
dotsafe scan src/ config/      # some paths
dotsafe scan --install-hook    # a pre-commit hook that runs \`dotsafe scan --staged\`
`}</CodeBlock>
    </DocsArticle>
  );
}
