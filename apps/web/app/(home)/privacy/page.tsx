import { docsContent } from '@/lib/docs-content';
import { pageMetadata } from '@/lib/seo';
import { LegalPage } from '@/components/LegalPage';

const { title, description } = docsContent('/privacy');
export const metadata = pageMetadata({ title, description, path: '/privacy', markdown: '/privacy.md' });

export default function PrivacyPage() {
  return <LegalPage href="/privacy" />;
}
