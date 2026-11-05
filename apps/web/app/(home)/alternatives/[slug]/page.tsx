import { ALTERNATIVES, sectionPaths } from '@/lib/content-pages';
import { docsContent } from '@/lib/docs-content';
import { pageMetadata } from '@/lib/seo';
import { MarkdownPage } from '@/components/MarkdownPage';

// One static page per slug in lib/content-pages.ts; anything else is a 404.
export const dynamicParams = false;

export function generateStaticParams() {
  return sectionPaths(ALTERNATIVES).map((path) => ({ slug: path.slice(`${ALTERNATIVES.href}/`.length) }));
}

export async function generateMetadata({ params }: PageProps<'/alternatives/[slug]'>) {
  const href = `${ALTERNATIVES.href}/${(await params).slug}`;
  const { title, description } = docsContent(href);
  return pageMetadata({ title, description, path: href, markdown: `${href}.md` });
}

export default async function Page({ params }: PageProps<'/alternatives/[slug]'>) {
  const href = `${ALTERNATIVES.href}/${(await params).slug}`;
  return <MarkdownPage href={href} eyebrow="Alternatives" section={{ name: 'Alternatives', href: ALTERNATIVES.href }} cta />;
}
