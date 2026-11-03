import { docsMetadata } from '@/lib/seo';
import { CodeBlock } from '@/components/docs/CodeBlock';
import { DocsArticle } from '@/components/docs/DocsArticle';

export const metadata = docsMetadata('/docs/storage');

export default function StoragePage() {
  return (
    <DocsArticle href="/docs/storage">
      <p>
        A vault is a folder of encrypted files. dotsafe never runs a sync server: you choose where the vault lives, and
        whatever stores it only ever sees ciphertext. Changes made on two Macs at once are merged, not overwritten.
      </p>

      <h2>A synced folder</h2>
      <p>
        The default is <code>iCloud Drive/dotsafe</code>. Any folder that syncs works: Google Drive, Dropbox, OneDrive,
        Syncthing.
      </p>
      <CodeBlock>{`dotsafe init --path ~/Dropbox/dotsafe`}</CodeBlock>

      <h2>An S3-compatible bucket</h2>
      <p>
        Cloudflare R2, AWS S3, Backblaze B2 and MinIO all work. Each Mac keeps an encrypted copy, so dotsafe works
        offline and syncs when it&apos;s back online. The app syncs in the background, and <code>dotsafe sync</code>{' '}
        syncs now.
      </p>
      <CodeBlock>{`
dotsafe init --bucket my-vaults --endpoint https://<account>.r2.cloudflarestorage.com
dotsafe init --bucket my-vaults --endpoint https://s3.us-east-1.amazonaws.com --region us-east-1
dotsafe sync                  # exchange changes now
`}</CodeBlock>
      <p>
        The access key is kept in the macOS keychain, never in a config file. For CI, set{' '}
        <code>DOTSAFE_S3_ACCESS_KEY_ID</code> and <code>DOTSAFE_S3_SECRET_ACCESS_KEY</code> instead. Give that key access
        to this one bucket only.
      </p>

      <h2>WebDAV</h2>
      <p>For Nextcloud, ownCloud or a NAS:</p>
      <CodeBlock>{`dotsafe init --webdav https://cloud.example.com/remote.php/dav/files/me/dotsafe/ --user me`}</CodeBlock>
      <p>
        The password is asked for and kept in the keychain, or read from <code>DOTSAFE_WEBDAV_PASSWORD</code>.
      </p>

      <h2>Backups</h2>
      <CodeBlock>{`dotsafe backup --to ~/Backups/dotsafe`}</CodeBlock>
      <p>
        A backup is a dated, still-encrypted copy of the vault. The folder is remembered for next time, and the last 10
        backups are kept.
      </p>

      <h2>Moving the vault</h2>
      <CodeBlock>{`
dotsafe move --path ~/Dropbox/dotsafe
dotsafe move --bucket my-vaults --endpoint https://<account>.r2.cloudflarestorage.com
`}</CodeBlock>
      <p>
        The vault is copied and this Mac switches to it. The old copy stays until you delete it. Other Macs connect to
        the new location with <code>dotsafe init</code>.
      </p>
    </DocsArticle>
  );
}
