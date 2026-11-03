import { docsMetadata } from '@/lib/seo';
import { CodeBlock } from '@/components/docs/CodeBlock';
import { DocsArticle } from '@/components/docs/DocsArticle';

export const metadata = docsMetadata('/docs/agents');

export default function AgentsPage() {
  return (
    <DocsArticle href="/docs/agents">
      <p>
        Coding agents need your keys to run tests, migrations and dev servers. With dotsafe they run those commands
        only after you approve, and no request an agent makes ever returns a secret.
      </p>

      <h2>Set up</h2>
      <CodeBlock>{`
claude mcp add dotsafe -- dotsafe mcp     # Claude Code
dotsafe mcp setup                         # the config for Cursor and other MCP agents
`}</CodeBlock>
      <p>
        <code>dotsafe mcp</code> holds no keys. It passes requests to the dotsafe app, which must be running and
        unlocked. Without the app, run <code>dotsafe daemon</code> in a terminal and approve requests there.
      </p>

      <h2>What an agent can do</h2>
      <ul>
        <li>List projects, environments and variable names. Never values.</li>
        <li>Ask to run a command with an environment&apos;s variables.</li>
        <li>
          Add a random secret, such as <code>JWT_SECRET</code>, without seeing the value it gets.
        </li>
      </ul>

      <h2>Approving a command</h2>
      <p>
        When an agent asks to run something, the app shows the exact command and which variables it gets. Allow it once
        or for a while, and confirm with Touch ID. Commands that would just print values, like <code>env</code> or{' '}
        <code>printenv</code>, are refused outright.
      </p>
      <p>
        The agent gets the output back with every secret value replaced by <code>[dotsafe:NAME]</code>, including
        common encodings of it.
      </p>

      <h2>The secret proxy</h2>
      <p>
        Redacting output isn&apos;t enough if the command itself could send a key somewhere. For OpenAI and Anthropic
        keys, dotsafe goes further: the command gets a placeholder key and a local URL, and dotsafe&apos;s proxy adds
        the real key on the way to the one HTTPS address it belongs to. The process never holds it.
      </p>
      <p>The same proxy protects your own dev servers:</p>
      <CodeBlock>{`dotsafe run --proxy -- bun dev`}</CodeBlock>
      <p>Add other APIs as proxy profiles, in Settings → Agents or with the CLI:</p>
      <CodeBlock>{`
dotsafe proxy add Mistral \\
  --key-var MISTRAL_API_KEY \\
  --base-url-var MISTRAL_BASE_URL \\
  --upstream https://api.mistral.ai \\
  --base-path /v1
dotsafe proxy list
dotsafe proxy remove Mistral
`}</CodeBlock>
      <p>
        Profiles are stored in the vault, never in a file in your repo, so an agent can&apos;t point a key at a server
        of its own.
      </p>

      <h2>Limits</h2>
      <p>
        A command you approve holds its secrets in its environment, and can do what it likes with them. Redaction
        catches accidents, like a stack trace that prints a key, not a command written to get around it. Read what
        you approve, and keep grants short. Keys behind the proxy are the exception: the process never has them.
      </p>

      <h2>Audit log</h2>
      <p>
        Every agent request is written to an encrypted audit log in the vault, with names only. It covers every device.
      </p>
      <CodeBlock>{`dotsafe audit --limit 50`}</CodeBlock>
    </DocsArticle>
  );
}
