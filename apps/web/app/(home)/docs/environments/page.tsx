import Link from 'next/link';
import { docsMetadata } from '@/lib/seo';
import { CodeBlock } from '@/components/docs/CodeBlock';
import { DocsArticle } from '@/components/docs/DocsArticle';

export const metadata = docsMetadata('/docs/environments');

export default function EnvironmentsPage() {
  return (
    <DocsArticle href="/docs/environments">
      <p>
        A vault holds projects, and each project holds environments such as development, staging and production. Every
        command works on the environment linked to the current directory, usually development. Pass{' '}
        <code>-e &lt;env&gt;</code> to use another.
      </p>

      <h2>Projects</h2>
      <p>
        A project is created the first time you write to it from a directory, along with a <code>.dotsafe.json</code>{' '}
        link file. Commit that file. To attach a directory to an existing project, or change its default environment:
      </p>
      <CodeBlock>{`
dotsafe link my-api                    # this directory now uses the my-api project
dotsafe link my-api -e staging         # and staging by default
`}</CodeBlock>

      <h2>Inheritance</h2>
      <p>
        An environment can inherit another one in the same project. It gets all of the parent&apos;s variables, and its
        own values win.
      </p>
      <CodeBlock>{`
dotsafe inherit production -e staging     # staging starts from production
dotsafe set -e staging LOG_LEVEL=debug    # and overrides one value
dotsafe unset -e staging --hide SENTRY_DSN  # drops one it shouldn't have
dotsafe inherit --none -e staging         # stop inheriting
`}</CodeBlock>
      <p>
        <code>unset</code> on its own removes the environment&apos;s own value, so the inherited one applies again.{' '}
        <code>--hide</code> removes the variable entirely. In the app, inherited and overridden variables are marked in
        the list.
      </p>

      <h2>References</h2>
      <p>
        A value can include other variables from the same environment with <code>{'${{ NAME }}'}</code>. They&apos;re
        filled in when the value is read, so changing <code>DB_HOST</code> updates <code>DB_URL</code> too.
      </p>
      <CodeBlock>{`
dotsafe set DB_HOST=db.internal
dotsafe set 'DB_URL=postgres://\${{ DB_HOST }}/app'   # single quotes, so the shell leaves it alone
`}</CodeBlock>
      <p>
        A reference to a variable that doesn&apos;t exist is an error, never an empty string, so a typo can&apos;t
        quietly reach your app.
      </p>

      <h2>In the app</h2>
      <ul>
        <li>
          <kbd>⌘K</kbd> finds a variable in any project and copies it, or jumps to an environment.
        </li>
        <li>
          <kbd>⌥⌘K</kbd> or the menu bar icon does the same from any other app.
        </li>
        <li>Copied values are cleared from the clipboard after 30 seconds.</li>
      </ul>
      <p>
        To share one value between projects, make it a <Link href="/docs/secrets">secret</Link>.
      </p>
    </DocsArticle>
  );
}
