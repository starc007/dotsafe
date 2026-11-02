import { Nav } from '@/components/landing/Nav';
import { Footer } from '@/components/landing/Footer';
import { MotionRoot } from '@/components/landing/MotionRoot';

/** The landing page's own frame: light only. */
export default function Layout({ children }: LayoutProps<'/'>) {
  return (
    <MotionRoot>
      <div className="landing flex min-h-screen flex-col">
        <Nav />
        <main className="flex-1">{children}</main>
        <Footer />
      </div>
    </MotionRoot>
  );
}
