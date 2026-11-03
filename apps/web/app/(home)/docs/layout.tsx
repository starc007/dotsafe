import { DocsMobileNav } from '@/components/docs/DocsMobileNav';
import { DocsSidebar } from '@/components/docs/DocsSidebar';

/** Docs: the page list pinned on the left, the page (and its own "On this
 * page" on wide screens) on the right. */
export default function DocsLayout({ children }: LayoutProps<'/docs'>) {
  return (
    <div className="mx-auto grid max-w-6xl gap-8 px-4 pt-24 pb-24 sm:px-6 sm:pt-28 lg:grid-cols-[14rem_minmax(0,1fr)] lg:gap-14 lg:pt-32">
      <aside className="hidden lg:block">
        <DocsSidebar className="sticky top-28 -ml-3 max-h-[calc(100vh-8rem)] overflow-y-auto pb-8" />
      </aside>
      <DocsMobileNav />
      <div className="min-w-0">{children}</div>
    </div>
  );
}
