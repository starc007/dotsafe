import { ALTERNATIVES } from '@/lib/content-pages';
import { pageMetadata } from '@/lib/seo';
import { ContentHub } from '@/components/ContentHub';

export const metadata = pageMetadata({
  title: ALTERNATIVES.title,
  description: ALTERNATIVES.description,
  path: ALTERNATIVES.href,
  markdown: `${ALTERNATIVES.href}.md`,
});

export default function Hub() {
  return <ContentHub section={ALTERNATIVES} />;
}
