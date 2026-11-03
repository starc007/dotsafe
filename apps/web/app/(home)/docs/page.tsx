import { docsMetadata } from '@/lib/seo';
import { DocsArticle } from '@/components/docs/DocsArticle';
import { DocsCards } from '@/components/docs/DocsCards';

export const metadata = docsMetadata('/docs');

export default function DocsPage() {
  return (
    <DocsArticle href="/docs">
      <DocsCards />
    </DocsArticle>
  );
}
