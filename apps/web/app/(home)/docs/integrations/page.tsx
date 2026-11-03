import Link from 'next/link';
import { docsMetadata } from '@/lib/seo';
import { CodeBlock } from '@/components/docs/CodeBlock';
import { DocsArticle } from '@/components/docs/DocsArticle';

export const metadata = docsMetadata('/docs/integrations');

export default function IntegrationsPage() {
  return (
    <DocsArticle href="/docs/integrations">
      <p>
        A <Link href="/docs/secrets">secret</Link> can have a <strong>provider</strong>, the service that issues its
        keys, and any number of <strong>targets</strong>, the places its value is deployed. With both set, one rotation
        asks the provider for a new key, makes it live in every linked environment and pushes it to your deploys.
      </p>
      <p>
        dotsafe calls these services from your Mac, with admin tokens you give it. The tokens are stored in the vault,
        encrypted like everything else. Set it all up in Settings → Integrations, or with the CLI below.
      </p>

      <h2>Supported services</h2>
      <table>
        <thead>
          <tr>
            <th>Service</th>
            <th>Name</th>
            <th>What dotsafe does</th>
          </tr>
        </thead>
        <tbody>
          <tr>
            <td>OpenAI</td>
            <td>
              <code>openai</code>
            </td>
            <td>Creates and revokes keys for a project</td>
          </tr>
          <tr>
            <td>Cloudflare API tokens</td>
            <td>
              <code>cloudflare_token</code>
            </td>
            <td>Rolls a token&apos;s value</td>
          </tr>
          <tr>
            <td>AWS IAM</td>
            <td>
              <code>aws_iam</code>
            </td>
            <td>Creates and deletes a user&apos;s access keys</td>
          </tr>
          <tr>
            <td>Anthropic</td>
            <td>
              <code>anthropic</code>
            </td>
            <td>Checks a key you made in the console, then switches the old one off</td>
          </tr>
          <tr>
            <td>Stripe</td>
            <td>
              <code>stripe</code>
            </td>
            <td>Checks a key you made in the dashboard; you revoke the old one there</td>
          </tr>
          <tr>
            <td>Vercel</td>
            <td>
              <code>vercel</code>
            </td>
            <td>Target: sets the variable on a project</td>
          </tr>
          <tr>
            <td>Cloudflare Workers</td>
            <td>
              <code>cloudflare_workers</code>
            </td>
            <td>Target: sets a Worker secret</td>
          </tr>
        </tbody>
      </table>

      <h2>Connect</h2>
      <CodeBlock>{`
dotsafe connect openai --set project_id=proj_…   # asks for the admin token
dotsafe connect vercel --name web
dotsafe connections                              # what's connected
dotsafe reconnect web                            # give a connection a new token
dotsafe disconnect web
`}</CodeBlock>
      <p>
        An admin token goes into the app once, when you connect, and never comes back out. Give it only the access
        dotsafe needs.
      </p>

      <h2>Set a provider and targets</h2>
      <CodeBlock>{`
dotsafe secret provider OPENAI_API_KEY --connection OpenAI
dotsafe secret target add OPENAI_API_KEY --connection web --set project=my-app
dotsafe secret push OPENAI_API_KEY        # push the current value to every target now
`}</CodeBlock>

      <h2>Rotate</h2>
      <CodeBlock>{`
dotsafe secret rotate OPENAI_API_KEY      # new key from OpenAI, live everywhere, pushed to Vercel
dotsafe secret retire OPENAI_API_KEY      # revoke the old key at OpenAI
`}</CodeBlock>
      <p>Each rotation happens in an order that never leaves you without a working key:</p>
      <ol>
        <li>The new key is saved to the vault first.</li>
        <li>dotsafe checks that it works.</li>
        <li>It goes live in every linked environment and is pushed to each target.</li>
        <li>
          Where the provider allows it, the old key keeps working until you retire it, or is revoked at once with{' '}
          <code>--leaked</code>.
        </li>
      </ol>

      <h3>Anthropic and Stripe</h3>
      <p>
        Their keys can only be made in their own console. <code>dotsafe secret rotate</code> shows the steps, asks for
        the new key (or reads it with <code>--stdin</code>), checks it, makes it live and pushes it.
      </p>
    </DocsArticle>
  );
}
