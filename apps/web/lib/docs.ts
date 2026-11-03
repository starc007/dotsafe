/** Every docs page, in reading order. The sidebar, the next and previous
 * links, page metadata and the sitemap all come from this list, so a new page
 * is a route under app/(home)/docs plus an entry here. */
export type DocsPage = { href: string; title: string; description: string };

export const DOCS: { title: string; pages: DocsPage[] }[] = [
  {
    title: 'Getting started',
    pages: [
      { href: '/docs', title: 'Introduction', description: 'What dotsafe is and how it keeps your secrets.' },
      { href: '/docs/install', title: 'Install', description: 'Download the app and the CLI, and activate your license.' },
      { href: '/docs/quick-start', title: 'Quick start', description: 'Create a vault, import a .env file and run your app.' },
    ],
  },
  {
    title: 'Guides',
    pages: [
      {
        href: '/docs/environments',
        title: 'Projects and environments',
        description: 'Projects, environments, inheritance and references between variables.',
      },
      {
        href: '/docs/secrets',
        title: 'Secrets and rotation',
        description: 'Share one value across environments, rotate it in one step and keep its history.',
      },
      {
        href: '/docs/integrations',
        title: 'Integrations',
        description: 'Rotate keys through OpenAI, Cloudflare and AWS, and push them to Vercel and Workers.',
      },
      {
        href: '/docs/agents',
        title: 'AI agents',
        description: 'Let Claude Code, Cursor and other agents run commands with secrets they never see.',
      },
      {
        href: '/docs/storage',
        title: 'Storage and sync',
        description: 'Keep the vault in a synced folder, an S3 or R2 bucket, or WebDAV.',
      },
      {
        href: '/docs/devices',
        title: 'Devices',
        description: 'Touch ID unlock, adding a Mac with a code, and removing a lost device.',
      },
      {
        href: '/docs/import-export',
        title: 'Import and export',
        description: 'Bring variables in from .env files and other secret managers, and keep them out of git.',
      },
    ],
  },
  {
    title: 'Reference',
    pages: [
      { href: '/docs/cli', title: 'CLI', description: 'Every dotsafe command and its main options.' },
      {
        href: '/docs/security',
        title: 'Security',
        description: 'How the vault is encrypted, what the app and agents can see, and what dotsafe never stores.',
      },
    ],
  },
];

const FLAT = DOCS.flatMap((group) => group.pages);

export function docsPage(href: string): DocsPage {
  const page = FLAT.find((p) => p.href === href);
  if (!page) throw new Error(`No docs page at ${href}; add it to lib/docs.ts`);
  return page;
}

/** The pages before and after this one, for the links at the bottom. */
export function docsNeighbours(href: string) {
  const i = FLAT.findIndex((p) => p.href === href);
  return { prev: FLAT[i - 1], next: FLAT[i + 1] };
}

export const DOCS_PATHS = FLAT.map((p) => p.href);
