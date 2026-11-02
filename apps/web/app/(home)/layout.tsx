import { Nav } from '@/components/landing/Nav';
import { Footer } from '@/components/landing/Footer';
import { MotionRoot } from '@/components/landing/MotionRoot';

/** The site's frame: nav and footer around the landing page and the docs. */
export default function Layout({ children }: LayoutProps<'/'>) {
  return (
    <MotionRoot>
      <div className="flex min-h-screen flex-col">
        <Nav />
        <main className="flex-1">{children}</main>
        <Footer />
      </div>
    </MotionRoot>
  );
}
