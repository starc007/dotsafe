import { docsContent } from '@/lib/docs-content';
import { pageMetadata } from '@/lib/seo';
import { MarkdownPage } from '@/components/MarkdownPage';

const { title, description } = docsContent('/terms');
export const metadata = pageMetadata({ title, description, path: '/terms', markdown: '/terms.md' });

export default function TermsPage() {
  return <MarkdownPage href="/terms" eyebrow="Legal" />;
}
