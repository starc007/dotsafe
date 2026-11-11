import { DOCS } from '@/lib/docs';
import { docsContent } from '@/lib/docs-content';

/** The guides and reference as cards, under the introduction. */
export function DocsCards() {
  const pages = DOCS.filter((g) => g.title !== 'Getting started').flatMap((g) => g.pages);
  return (
    <section aria-labelledby="docs-cards" className="mt-14">
      <h2 id="docs-cards" className="text-[20px] font-medium tracking-[-0.01em]">
        Guides and reference
      </h2>
      <ul className="mt-5 grid gap-3 sm:grid-cols-2">
        {pages.map((page) => {
          const Icon = page.icon;
          return (
            <li key={page.href}>
              <a
                href={page.href}
                className="flex h-full gap-3 rounded-2xl border border-ds-line p-4 transition-colors duration-200 hover:border-ds-fg/20"
              >
                <span className="flex size-8 shrink-0 items-center justify-center rounded-full bg-ds-shell">
                  <Icon aria-hidden className="size-4 text-ds-fg/70" />
                </span>
                <span className="min-w-0">
                  <span className="block text-sm font-medium">{page.title}</span>
                  <span className="mt-0.5 block text-[13px] leading-snug text-pretty text-ds-muted">
                    {docsContent(page.href).description}
                  </span>
                </span>
              </a>
            </li>
          );
        })}
      </ul>
    </section>
  );
}
