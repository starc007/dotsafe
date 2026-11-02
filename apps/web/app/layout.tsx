import type { Metadata } from 'next';
import './global.css';
import { Inter } from 'next/font/google';

const inter = Inter({
  subsets: ['latin'],
});

export const metadata: Metadata = {
  title: 'dotsafe',
  description: 'A local-first, zero-knowledge secret manager for developers.',
};

export default function Layout({ children }: LayoutProps<'/'>) {
  return (
    <html lang="en" className={inter.className}>
      <body className="flex min-h-screen flex-col">{children}</body>
    </html>
  );
}
