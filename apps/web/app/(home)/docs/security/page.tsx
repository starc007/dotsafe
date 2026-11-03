import { links } from '@/lib/site';
import { docsMetadata } from '@/lib/seo';
import { DocsArticle } from '@/components/docs/DocsArticle';

export const metadata = docsMetadata('/docs/security');

export default function SecurityPage() {
  return (
    <DocsArticle href="/docs/security">
      <p>
        Your secrets are encrypted on your device. dotsafe never sees them, and neither does your storage provider. The
        code is open source, so you can check every claim on this page.
      </p>

      <h2>Encryption</h2>
      <ul>
        <li>
          Names and values are encrypted with <strong>XChaCha20-Poly1305</strong>, under keys derived from the vault
          key.
        </li>
        <li>
          The vault key is sealed with a key derived from your master password by <strong>Argon2id</strong>, and with
          your recovery key. Neither is ever stored.
        </li>
        <li>
          Every record is authenticated and bound to its vault and its own id, so it can&apos;t be changed or moved
          without being caught.
        </li>
        <li>
          A sealed vault head records the newest version of every record, so a Mac notices when storage serves an older
          copy of one than it has already seen.
        </li>
      </ul>

      <h2>Who can see what</h2>
      <table>
        <thead>
          <tr>
            <th>Who</th>
            <th>Can</th>
            <th>Can&apos;t</th>
          </tr>
        </thead>
        <tbody>
          <tr>
            <td>Your storage provider</td>
            <td>See encrypted files, their count and sizes; delete them</td>
            <td>Read names or values, or change them unnoticed</td>
          </tr>
          <tr>
            <td>dotsafe (the license server)</td>
            <td>Check or revoke a license</td>
            <td>Read or change any vault, or lock you out of one</td>
          </tr>
          <tr>
            <td>A thief with your locked Mac</td>
            <td>Copy the encrypted files</td>
            <td>Unlock them without your password or your device key</td>
          </tr>
          <tr>
            <td>A device you removed</td>
            <td>Keep what it already read</td>
            <td>Read anything written after the removal</td>
          </tr>
          <tr>
            <td>An AI agent</td>
            <td>See names; ask to run commands</td>
            <td>Get a value from dotsafe, run anything you didn&apos;t approve, or read a proxied key</td>
          </tr>
        </tbody>
      </table>

      <h2>On your Mac</h2>
      <ul>
        <li>The vault locks after 15 minutes without activity, by default.</li>
        <li>Device keys and storage credentials live in the macOS keychain. Device keys are read only after Touch ID.</li>
        <li>
          The app&apos;s interface gets a value only when you reveal or edit one. Lists carry names only, and copying
          happens outside it. Copied values are cleared from the clipboard after 30 seconds.
        </li>
        <li>The interface can&apos;t load remote content, or reach your files or clipboard.</li>
        <li>Updates are signed, checked before they install, and only download when you click.</li>
      </ul>

      <h2>What dotsafe doesn&apos;t protect against</h2>
      <ul>
        <li>
          Malware running as you while the vault is unlocked. It can read memory or log your password. No local secret
          manager can stop that; auto-lock and short agent grants limit the window.
        </li>
        <li>A command you approved, once it holds a secret. It can do what it likes with it.</li>
        <li>Someone learning that you have a vault, or roughly how many records are in it.</li>
      </ul>

      <h2>Read more</h2>
      <ul>
        <li>
          <a href={`${links.github}/blob/main/docs/architecture/threat-model.md`}>Threat model</a>
        </li>
        <li>
          <a href={`${links.github}/blob/main/docs/vault-format.md`}>Vault format</a>
        </li>
      </ul>
    </DocsArticle>
  );
}
