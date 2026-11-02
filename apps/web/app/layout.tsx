import type { Metadata } from 'next';
import './global.css';
import { Inter } from 'next/font/google';

const inter = Inter({
  subsets: ['latin'],
});

export const metadata: Metadata = {
  metadataBase: new URL('https://dotsafe.dev'),
  title: 'dotsafe',
  description: 'A local-first, zero-knowledge secret manager for developers.',
  openGraph: { siteName: 'dotsafe', type: 'website' },
  twitter: { card: 'summary_large_image' },
};

export default function Layout({ children }: LayoutProps<'/'>) {
  return (
    <html lang="en" className={inter.className}>
      <body className="flex min-h-screen flex-col">{children}</body>
    </html>
  );
}
