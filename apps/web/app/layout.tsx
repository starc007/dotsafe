import type { Metadata } from 'next';
import './global.css';
import { Inter } from 'next/font/google';
import { site } from '@/lib/site';
import { TracwellAnalytics } from '@/components/TracwellAnalytics';

const inter = Inter({
  subsets: ['latin'],
});

export const metadata: Metadata = {
  metadataBase: new URL(site.url),
  title: { default: site.title, template: `%s · ${site.name}` },
  description: site.description,
  applicationName: site.name,
  alternates: { canonical: '/' },
  openGraph: { siteName: site.name, type: 'website', url: '/', title: site.title, description: site.description },
  twitter: { card: 'summary_large_image', title: site.title, description: site.description },
};

export default function Layout({ children }: LayoutProps<'/'>) {
  return (
    <html lang="en" className={inter.className}>
      <body className="flex min-h-screen flex-col">
        {children}
        {/* After the page, so its effect runs after the page's (see the component). */}
        <TracwellAnalytics />
      </body>
    </html>
  );
}
