/**
 * Writes the site's files for AI agents into public/, before every dev and
 * build: Markdown copies of the home, pricing and changelog pages, llms.txt,
 * llms-full.txt (every docs page in one file) and an RSS feed of releases.
 *
 * They're built from the same data as the pages (lib/), so they can't drift.
 * The docs pages' own .md files are written by hand in public/docs and only
 * read here. The outputs are gitignored.
 *
 *   bun scripts/agent-files.ts
 */
import { writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { RELEASES } from '@/lib/changelog';
import { DOCS } from '@/lib/docs';
import { docsContent } from '@/lib/docs-content';
import { FAQ } from '@/lib/faq';
import { PLAN } from '@/lib/pricing';
import { links, site } from '@/lib/site';

const out = (name: string, text: string) => writeFileSync(join(process.cwd(), 'public', name), `${text.trim()}\n`);
const abs = (path: string) => (path.startsWith('http') ? path : `${site.url}${path}`);
/** Root-relative links become absolute, for files read outside the site. */
const absolutize = (md: string) => md.replace(/\]\(\//g, `](${site.url}/`);

const SUMMARY =
  'dotsafe is a local-first, zero-knowledge secret manager for developers. Secrets are encrypted on your Mac and synced through storage you already own (iCloud Drive, Google Drive, Dropbox, an S3 or R2 bucket, or WebDAV). There is no dotsafe server holding your vault. It ships as a macOS app and a `dotsafe` CLI that share one vault, and AI agents can run commands with your secrets through its MCP server after you approve them, without dotsafe ever handing them a value.';

const docs = DOCS.map((group) => ({
  ...group,
  pages: group.pages.map((p) => ({ ...p, ...docsContent(p.href) })),
}));

const faq = FAQ.map(({ q, a }) => `### ${q}\n\n${a}`).join('\n\n');

const pricing = `${PLAN.price} ${PLAN.term} (instead of ${PLAN.was}). ${PLAN.blurb}

${PLAN.perks.map((perk) => `- ${perk}`).join('\n')}

Buy on the [pricing page](${site.url}/pricing). The license key arrives by email; activate it in the app or with \`dotsafe license activate <key>\`. There is no free version.`;

out(
  'index.md',
  `# ${site.name}

> ${site.description}

${SUMMARY}

- [Download for macOS](${links.download})
- [Docs](${site.url}/docs.md)
- [Pricing](${site.url}/pricing.md)
- [Changelog](${site.url}/changelog.md)
- [Source on GitHub](${links.github}) (MIT license)

## What it does

- **Projects and environments.** Variables per environment, with inheritance and \`\${{ NAME }}\` references. \`dotsafe run -- <command>\` injects them, so there's no .env file to leak.
- **Secrets and rotation.** Link one secret into every environment that uses it and rotate it once, through OpenAI, Cloudflare or AWS, with pushes to Vercel and Cloudflare Workers.
- **AI agents.** Claude Code, Cursor and other MCP agents can list names and run commands you approve with Touch ID. Output comes back with values replaced by \`[dotsafe:NAME]\`, and OpenAI and Anthropic keys go through a local proxy so the process never holds them.
- **Storage you own.** iCloud Drive, Google Drive, Dropbox, any synced folder, S3-compatible buckets or WebDAV. Works offline.
- **Devices.** Touch ID unlock, adding a Mac with a one-time code, and removing a lost one (the vault is re-keyed).

## Security

Names and values are encrypted with XChaCha20-Poly1305 under keys derived from your master password with Argon2id. The only dotsafe server handles licenses and never sees a vault. See [Security](${site.url}/docs/security.md).

## Pricing

${pricing}

## FAQ

${faq}
`,
);

out(
  'pricing.md',
  `# Pricing

> One ${site.name} license, ${PLAN.price} once, for up to 2 Macs.

${pricing}

## FAQ

${faq}
`,
);

out(
  'changelog.md',
  `# Changelog

> New features, fixes and improvements in each ${site.name} release.

${RELEASES.map(
  (r) => `## ${r.version}: ${r.title} (${r.date ?? 'upcoming'})

${r.summary ? `${r.summary}\n\n` : ''}${r.changes.map((c) => `- ${c}`).join('\n')}`,
).join('\n\n')}
`,
);

const list = (pages: { href: string; title: string; description: string }[]) =>
  pages.map((p) => `- [${p.title}](${abs(`${p.href}.md`)}): ${p.description}`).join('\n');

out(
  'llms.txt',
  `# ${site.name}

> ${SUMMARY}

Every page on this site is also plain Markdown: add \`.md\` to its URL (the home page is ${site.url}/index.md). [llms-full.txt](${site.url}/llms-full.txt) has every docs page in one file.

${docs.map((g) => `## ${g.title}\n\n${list(g.pages)}`).join('\n\n')}

## More

- [Home](${site.url}/index.md): What dotsafe is, in short
- [Pricing](${site.url}/pricing.md): The one plan and the FAQ
- [Changelog](${site.url}/changelog.md): What changed in each release
- [Source code](${links.github}): The app, the CLI and the vault format, MIT licensed
`,
);

out(
  'llms-full.txt',
  `# ${site.name} docs

> ${SUMMARY}

${docs
  .flatMap((g) => g.pages)
  .map((p) => `<!-- ${abs(p.href)} -->\n\n${absolutize(p.markdown.trim())}`)
  .join('\n\n---\n\n')}
`,
);

const xml = (s: string) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
const released = RELEASES.filter((r) => r.date);
out(
  'changelog.xml',
  `<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0" xmlns:atom="http://www.w3.org/2005/Atom">
  <channel>
    <title>${site.name} changelog</title>
    <link>${site.url}/changelog</link>
    <description>New features, fixes and improvements in each ${site.name} release.</description>
    <language>en</language>
    <atom:link href="${site.url}/changelog.xml" rel="self" type="application/rss+xml" />
${released
  .map(
    (r) => `    <item>
      <title>${xml(`${r.version}: ${r.title}`)}</title>
      <link>${site.url}/changelog#v${r.version}</link>
      <guid isPermaLink="false">${site.name}-${r.version}</guid>
      <pubDate>${new Date(r.date!).toUTCString()}</pubDate>
      <description>${xml([r.summary, ...r.changes].filter(Boolean).join(' '))}</description>
    </item>`,
  )
  .join('\n')}
  </channel>
</rss>`,
);

console.log('agent files: index.md, pricing.md, changelog.md, changelog.xml, llms.txt, llms-full.txt');
