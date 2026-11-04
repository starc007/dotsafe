import { docsContent } from '@/lib/docs-content';
import { pageMetadata } from '@/lib/seo';
import { LegalPage } from '@/components/LegalPage';

const { title, description } = docsContent('/terms');
export const metadata = pageMetadata({ title, description, path: '/terms', markdown: '/terms.md' });

export default function TermsPage() {
  return <LegalPage href="/terms" />;
}
