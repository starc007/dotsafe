import { USE_CASES, sectionPaths } from '@/lib/content-pages';
import { docsContent } from '@/lib/docs-content';
import { pageMetadata } from '@/lib/seo';
import { MarkdownPage } from '@/components/MarkdownPage';

// One static page per slug in lib/content-pages.ts; anything else is a 404.
export const dynamicParams = false;

export function generateStaticParams() {
  return sectionPaths(USE_CASES).map((path) => ({ slug: path.slice(`${USE_CASES.href}/`.length) }));
}

export async function generateMetadata({ params }: PageProps<'/for/[slug]'>) {
  const href = `${USE_CASES.href}/${(await params).slug}`;
  const { title, description } = docsContent(href);
  return pageMetadata({ title, description, path: href, markdown: `${href}.md` });
}

export default async function Page({ params }: PageProps<'/for/[slug]'>) {
  const href = `${USE_CASES.href}/${(await params).slug}`;
  return <MarkdownPage href={href} eyebrow="Use cases" section={{ name: 'Use cases', href: USE_CASES.href }} cta />;
}
