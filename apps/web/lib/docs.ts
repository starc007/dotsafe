import type { LucideIcon } from 'lucide-react';
import {
  ArrowDownToLine,
  Bot,
  FileInput,
  HardDrive,
  KeyRound,
  Layers,
  Laptop,
  Plug,
  Rocket,
  ShieldCheck,
  SquareTerminal,
  BookOpen,
} from 'lucide-react';

/** Every docs page, in reading order. A page's text lives in Markdown at
 * `public/<href>.md` (served as is, for agents) and its HTML page is rendered
 * from that file. The sidebar, next and previous links and the sitemap come
 * from this list, so a new page is a .md file, a route and an entry here.
 * Keep public/llms.txt in step too. */
export type DocsPage = { href: string; title: string; icon: LucideIcon };

export const DOCS: { title: string; pages: DocsPage[] }[] = [
  {
    title: 'Getting started',
    pages: [
      { href: '/docs', title: 'Introduction', icon: BookOpen },
      { href: '/docs/install', title: 'Install', icon: ArrowDownToLine },
      { href: '/docs/quick-start', title: 'Quick start', icon: Rocket },
    ],
  },
  {
    title: 'Guides',
    pages: [
      { href: '/docs/environments', title: 'Projects and environments', icon: Layers },
      { href: '/docs/secrets', title: 'Secrets and rotation', icon: KeyRound },
      { href: '/docs/integrations', title: 'Integrations', icon: Plug },
      { href: '/docs/agents', title: 'AI agents', icon: Bot },
      { href: '/docs/storage', title: 'Storage and sync', icon: HardDrive },
      { href: '/docs/devices', title: 'Devices', icon: Laptop },
      { href: '/docs/import-export', title: 'Import and export', icon: FileInput },
    ],
  },
  {
    title: 'Reference',
    pages: [
      { href: '/docs/cli', title: 'CLI', icon: SquareTerminal },
      { href: '/docs/security', title: 'Security', icon: ShieldCheck },
    ],
  },
];

const FLAT = DOCS.flatMap((group) => group.pages.map((page) => ({ ...page, group: group.title })));

export function docsEntry(href: string) {
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
