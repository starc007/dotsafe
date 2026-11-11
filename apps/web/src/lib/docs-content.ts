import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { slugify } from '@/lib/slugify';

export type DocsHeading = { id: string; text: string };
export type DocsContent = {
  title: string;
  description: string;
  /** The whole file, as served at `<href>.md`. */
  markdown: string;
  /** What the page renders: everything after the title and description. */
  body: string;
  headings: DocsHeading[];
};

/** Reads a docs page's Markdown at build time (server only: it uses fs). Each file starts with
 * `# Title`, then `> description`, then the body. */
export function docsContent(href: string): DocsContent {
  const markdown = readFileSync(join(process.cwd(), 'public', `${href}.md`), 'utf8');
  const match = markdown.match(/^# (.+)\n+> (.+)\n+([\s\S]*)$/);
  if (!match) throw new Error(`public${href}.md must start with "# Title" and "> description"`);
  const [, title, description, body] = match;
  return { title, description, markdown, body, headings: headings(body) };
}

/** The `##` headings, skipping fenced code, for "On this page". */
function headings(body: string): DocsHeading[] {
  const found: DocsHeading[] = [];
  let fenced = false;
  for (const line of body.split('\n')) {
    if (line.startsWith('```')) fenced = !fenced;
    else if (!fenced && line.startsWith('## ')) {
      const text = line.slice(3).trim();
      found.push({ id: slugify(text), text });
    }
  }
  return found;
}
