import Link from 'next/link';
import { docsMetadata } from '@/lib/seo';
import { DocsArticle } from '@/components/docs/DocsArticle';

export const metadata = docsMetadata('/docs/cli');

type Command = { usage: string; does: string };

/** Mirrors `dotsafe --help`; keep it in step with crates/cli/src/main.rs. */
const GROUPS: { title: string; id: string; commands: Command[] }[] = [
  {
    title: 'Vault',
    id: 'vault',
    commands: [
      { usage: 'init [--path | --bucket | --webdav]', does: 'Create a vault, or connect this Mac to an existing one' },
      { usage: 'unlock [--for 8h]', does: 'Keep the vault key in the keychain for a while' },
      { usage: 'lock', does: 'Forget the key cached by unlock' },
      { usage: 'sync', does: 'Exchange changes with the bucket now' },
      { usage: 'backup [--to <dir>]', does: 'Save a dated, still-encrypted copy (the last 10 are kept)' },
      { usage: 'move [--path | --bucket | --webdav]', does: 'Move the vault; the old copy stays until you delete it' },
    ],
  },
  {
    title: 'Variables',
    id: 'variables',
    commands: [
      { usage: 'link [name] [-e env]', does: 'Link this directory to a project, creating it if needed' },
      { usage: 'set KEY[=VALUE]…', does: 'Set variables; a bare KEY asks for the value' },
      { usage: 'get KEY', does: "Print a variable's value" },
      { usage: 'list', does: 'List variable names' },
      { usage: 'unset KEY… [--hide]', does: 'Delete variables, or hide inherited ones' },
      { usage: 'inherit <parent> | --none', does: "Inherit another environment's variables" },
      { usage: 'run [--proxy] -- <command>', does: 'Run a command with the variables in its environment' },
      { usage: 'import [file] [--from <tool>]', does: 'Import a .env or JSON file, or from Doppler, Infisical, 1Password' },
      { usage: 'export [-o file] [--force]', does: 'Print .env lines, or write a gitignored, owner-only file' },
      { usage: 'scan [paths] [--staged | --install-hook]', does: 'Find vault values in files before they’re committed' },
    ],
  },
  {
    title: 'Secrets',
    id: 'secrets',
    commands: [
      { usage: 'secret list', does: "Secrets with their age and where they're used" },
      { usage: 'secret share KEY [--all]', does: 'Turn a variable into a secret other environments can link to' },
      { usage: 'secret link KEY SECRET', does: 'Link a variable to an existing secret' },
      { usage: 'secret unlink KEY', does: 'Give a linked variable its own copy again' },
      {
        usage: 'secret rotate SECRET [--generate | --leaked | --stdin]',
        does: 'New value everywhere it’s linked, through its provider if it has one',
      },
      { usage: 'secret retire SECRET [--by-hand]', does: 'Revoke the previous value' },
      { usage: 'secret rollback SECRET', does: 'Go back to the previous value' },
      { usage: 'secret history SECRET', does: 'Versions and notes, never values' },
      { usage: 'secret restore SECRET VERSION', does: 'Make an earlier version current again' },
      { usage: 'secret policy SECRET --every <days>', does: 'How often it should be rotated' },
      { usage: 'secret due', does: 'Secrets due for rotation' },
      { usage: 'secret queue [--rotate | --dismiss]', does: 'What to rotate after removing a device' },
      { usage: 'secret provider SECRET --connection <name>', does: 'Where new values come from' },
      { usage: 'secret target add SECRET --connection <name>', does: 'Where each new value is pushed' },
      { usage: 'secret push SECRET', does: 'Push the current value to every target now' },
    ],
  },
  {
    title: 'Integrations',
    id: 'integrations',
    commands: [
      { usage: 'connect <service> [--set key=value]', does: 'Connect a provider or deploy target' },
      { usage: 'connections', does: 'List connected accounts' },
      { usage: 'reconnect <name>', does: 'Give a connection a new admin token' },
      { usage: 'disconnect <name>', does: 'Remove a connection and its token' },
    ],
  },
  {
    title: 'Agents',
    id: 'agents',
    commands: [
      { usage: 'mcp', does: 'The MCP server agents talk to' },
      { usage: 'mcp setup', does: 'How to add dotsafe to Claude Code, Cursor and others' },
      { usage: 'daemon [--for 8h]', does: 'Serve agents without the app; approve in the terminal' },
      { usage: 'proxy list | add | remove', does: 'Proxy profiles for more API keys' },
      { usage: 'audit [--limit 30]', does: 'What agents asked for, from every device' },
    ],
  },
  {
    title: 'Devices and license',
    id: 'devices',
    commands: [
      { usage: 'device list', does: 'Devices that can open this vault' },
      { usage: 'device remove <name>', does: 'Remove one and switch the vault to a new key' },
      { usage: 'license activate <key>', does: 'Activate a license on this Mac' },
      { usage: 'license status', does: 'Show the license on this Mac' },
      { usage: 'license remove', does: "Free this Mac's activation" },
    ],
  },
];

export default function CliPage() {
  return (
    <DocsArticle href="/docs/cli">
      <p>
        Every command takes <code>--help</code>. Commands that work on variables take <code>-e &lt;env&gt;</code> for
        an environment other than the linked one.
      </p>
      {GROUPS.map((group) => (
        <section key={group.id}>
          <h2 id={group.id}>{group.title}</h2>
          <table className="mt-4">
            <tbody>
              {group.commands.map((c) => (
                <tr key={c.usage}>
                  <td className="w-1/2">
                    <code>dotsafe {c.usage}</code>
                  </td>
                  <td className="text-ds-muted">{c.does}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </section>
      ))}

      <h2 id="environment">Environment variables</h2>
      <table className="mt-4">
        <tbody>
          <tr>
            <td className="w-1/2">
              <code>DOTSAFE_PASSWORD</code>
            </td>
            <td className="text-ds-muted">
              The master password, for CI. Anything that can read the environment can read it, so prefer{' '}
              <code>unlock</code> on your own Mac.
            </td>
          </tr>
          <tr>
            <td>
              <code>DOTSAFE_S3_ACCESS_KEY_ID</code>, <code>DOTSAFE_S3_SECRET_ACCESS_KEY</code>
            </td>
            <td className="text-ds-muted">Bucket credentials, instead of the keychain</td>
          </tr>
          <tr>
            <td>
              <code>DOTSAFE_WEBDAV_PASSWORD</code>
            </td>
            <td className="text-ds-muted">The WebDAV password, instead of asking</td>
          </tr>
          <tr>
            <td>
              <code>DOTSAFE_CONFIG_DIR</code>
            </td>
            <td className="text-ds-muted">Use another config folder, for testing</td>
          </tr>
        </tbody>
      </table>
      <p>
        New here? Start with the <Link href="/docs/quick-start">quick start</Link>.
      </p>
    </DocsArticle>
  );
}
