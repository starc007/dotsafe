import { appendFileSync, existsSync, readdirSync } from 'node:fs';
import { join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

/**
 * An Astro integration that gives every page with a Markdown copy a `Link`
 * header naming it, the same as the `<link rel="alternate">` in its head, for
 * agents that read headers only. After the build it finds each `X.html` in
 * `dist/` with an `X.md` beside it (the home page is `index.md`) and appends
 * an exact rule for its address to `dist/_headers`.
 *
 * Exact rules, because a splat like `/docs/*` also matches the `.md` files,
 * and `_headers` can't take a header back off them without breaking the
 * other rules that match.
 */
export function linkHeaders() {
  return {
    name: 'link-headers',
    hooks: {
      'astro:build:done': ({ dir, logger }) => {
        const root = fileURLToPath(dir);
        const rules = htmlFiles(root)
          .map((file) => relative(root, file).replace(/\.html$/, ''))
          .filter((page) => existsSync(join(root, `${page}.md`)))
          .sort()
          .map((page) => {
            const path = page === 'index' ? '/' : `/${page}`;
            return `${path}\n  Link: </${page}.md>; rel="alternate"; type="text/markdown"`;
          });
        appendFileSync(join(root, '_headers'), `\n# Link headers, added at build (scripts/link-headers.mjs).\n${rules.join('\n')}\n`);
        logger.info(`Link headers for ${rules.length} pages`);
      },
    },
  };
}

function htmlFiles(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) return htmlFiles(path);
    return entry.name.endsWith('.html') ? [path] : [];
  });
}
