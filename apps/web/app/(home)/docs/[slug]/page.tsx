import { DOCS_PATHS } from '@/lib/docs';
import { docsMetadata } from '@/lib/seo';
import { DocsArticle } from '@/components/docs/DocsArticle';

// One static page per entry in lib/docs.ts; anything else is a 404.
export const dynamicParams = false;

export function generateStaticParams() {
  return DOCS_PATHS.filter((p) => p !== '/docs').map((p) => ({ slug: p.slice('/docs/'.length) }));
}

export async function generateMetadata({ params }: PageProps<'/docs/[slug]'>) {
  const { slug } = await params;
  return docsMetadata(`/docs/${slug}`);
}

export default async function DocsSlugPage({ params }: PageProps<'/docs/[slug]'>) {
  const { slug } = await params;
  return <DocsArticle href={`/docs/${slug}`} />;
}
