import Link from 'next/link';
import { docsMetadata } from '@/lib/seo';
import { CodeBlock } from '@/components/docs/CodeBlock';
import { DocsArticle } from '@/components/docs/DocsArticle';

export const metadata = docsMetadata('/docs/secrets');

export default function SecretsPage() {
  return (
    <DocsArticle href="/docs/secrets">
      <p>
        A plain variable belongs to one environment. A <strong>secret</strong> is a value that many environments, in
        any project, can link to. Rotate it once and every linked environment gets the new value. Secrets also keep
        their history, so a bad rotation can be undone.
      </p>

      <h2>Share a variable</h2>
      <CodeBlock>{`
dotsafe secret share STRIPE_KEY            # turn this variable into a secret
dotsafe secret share STRIPE_KEY --all      # and link every environment that had the same value
dotsafe secret link STRIPE_KEY STRIPE_KEY -e production   # link one more variable to it
dotsafe secret unlink STRIPE_KEY -e production            # give it its own copy again
dotsafe secret list                        # age and where each one is used
`}</CodeBlock>
      <p>
        Copies of a secret&apos;s value that aren&apos;t linked are flagged in the app, so they don&apos;t go stale after
        the next rotation.
      </p>

      <h2>Rotate</h2>
      <CodeBlock>{`
dotsafe secret rotate STRIPE_KEY                     # type the new value
dotsafe secret rotate JWT_SECRET --generate          # or have dotsafe make one
dotsafe secret rotate STRIPE_KEY --note "yearly"     # kept in the history
`}</CodeBlock>
      <p>
        The new value goes live everywhere at once. The old one is kept as the <em>previous</em> value until you retire
        it, so you have time to revoke it with the provider without downtime.
      </p>
      <CodeBlock>{`
dotsafe secret retire STRIPE_KEY        # once the old value is revoked
`}</CodeBlock>

      <h3>If a key leaked</h3>
      <CodeBlock>{`dotsafe secret rotate STRIPE_KEY --leaked`}</CodeBlock>
      <p>
        <code>--leaked</code> retires the old value right away instead of keeping it as previous. If the secret has a{' '}
        <Link href="/docs/integrations">provider</Link>, the old key is revoked at the provider too.
      </p>

      <h2>History, rollback and restore</h2>
      <CodeBlock>{`
dotsafe secret history STRIPE_KEY       # versions and notes, never values
dotsafe secret rollback STRIPE_KEY      # back to the previous value
dotsafe secret restore STRIPE_KEY 3     # make version 3 current again
`}</CodeBlock>

      <h2>Rotation policies</h2>
      <p>Give a secret a rotation interval and dotsafe tells you when it&apos;s due.</p>
      <CodeBlock>{`
dotsafe secret policy STRIPE_KEY --every 90   # days; 0 removes the policy
dotsafe secret due                            # what's due now
`}</CodeBlock>
      <p>The app sends a notification when a secret comes due, and marks it in the sidebar.</p>

      <h2>After removing a device</h2>
      <p>
        When you <Link href="/docs/devices">remove a device</Link>, every value it could read goes into a rotation queue.
        It stays in the sidebar until each one is rotated.
      </p>
      <CodeBlock>{`
dotsafe secret queue              # what's left
dotsafe secret queue --rotate     # rotate every queued secret that has a provider
`}</CodeBlock>
    </DocsArticle>
  );
}
