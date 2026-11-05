import { USE_CASES } from '@/lib/content-pages';
import { pageMetadata } from '@/lib/seo';
import { ContentHub } from '@/components/ContentHub';

export const metadata = pageMetadata({
  title: USE_CASES.title,
  description: USE_CASES.description,
  path: USE_CASES.href,
  markdown: `${USE_CASES.href}.md`,
});

export default function Hub() {
  return <ContentHub section={USE_CASES} />;
}
