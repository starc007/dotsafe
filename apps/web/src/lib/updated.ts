import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';

/** When a Markdown page last changed, as `YYYY-MM-DD`, for its "Updated" line,
 * `dateModified` and the sitemap's `<lastmod>` (build time only: it uses fs and
 * git). Search engines and AI answers weigh how fresh a page is.
 *
 * It's the last commit that touched `public/<href>.md`. A shallow clone would
 * give every file the same date, so there git isn't trusted. Without a usable
 * date from git, a "Checked on <date>." line in the page is used (the
 * alternatives pages have one), and otherwise there's no date at all: no date
 * is better than a wrong one. */
export function pageUpdated(href: string): string | undefined {
  const file = join('public', `${href}.md`);
  return fromGit(file) ?? fromCheckedOn(file);
}

/** "November 5, 2026", for the page. */
export function formatUpdated(date: string) {
  return new Date(`${date}T00:00:00Z`).toLocaleDateString('en-US', {
    month: 'long',
    day: 'numeric',
    year: 'numeric',
    timeZone: 'UTC',
  });
}

const git = (args: string[]) => execFileSync('git', args, { encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] }).trim();

let fullHistory: boolean | undefined;
function hasFullHistory() {
  try {
    fullHistory ??= git(['rev-parse', '--is-shallow-repository']) === 'false';
  } catch {
    fullHistory = false;
  }
  return fullHistory;
}

function fromGit(file: string) {
  if (!hasFullHistory()) return undefined;
  try {
    // Changed since the last commit: the edit is today's.
    if (git(['status', '--porcelain', '--', file])) return new Date().toISOString().slice(0, 10);
    return git(['log', '-1', '--format=%cs', '--', file]) || undefined;
  } catch {
    return undefined;
  }
}

function fromCheckedOn(file: string) {
  try {
    const match = readFileSync(join(process.cwd(), file), 'utf8').match(/^Checked on (\w+ \d{1,2}, \d{4})\.$/m);
    if (!match) return undefined;
    const date = new Date(`${match[1]} UTC`);
    return Number.isNaN(date.getTime()) ? undefined : date.toISOString().slice(0, 10);
  } catch {
    return undefined;
  }
}
