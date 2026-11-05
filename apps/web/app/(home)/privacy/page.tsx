import { docsContent } from '@/lib/docs-content';
import { pageMetadata } from '@/lib/seo';
import { MarkdownPage } from '@/components/MarkdownPage';

const { title, description } = docsContent('/privacy');
export const metadata = pageMetadata({ title, description, path: '/privacy', markdown: '/privacy.md' });

export default function PrivacyPage() {
  return <MarkdownPage href="/privacy" eyebrow="Legal" />;
}
