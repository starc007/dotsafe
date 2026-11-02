import type { ReactNode } from 'react';
import Link from 'next/link';
import { ArrowRight } from 'lucide-react';
import { cn } from '@/lib/cn';
import { Reveal } from '@/components/landing/Reveal';
import { Wallpaper, type Scene } from '@/components/landing/Wallpaper';

/** A feature as a wide row: the claim on one side, its live demo in a white
 * window on a painted wallpaper on the other. `flip` puts the demo first;
 * `bare` skips the window for demos that bring their own. */
export function FeatureRow({
  title,
  children,
  demo,
  scene,
  href,
  flip,
  bare,
}: {
  title: string;
  children: ReactNode;
  demo: ReactNode;
  scene: Scene;
  href?: string;
  flip?: boolean;
  bare?: boolean;
}) {
  return (
    <Reveal className="grid overflow-hidden rounded-3xl border border-ds-line p-2 md:grid-cols-5">
      <div className={cn('flex flex-col justify-end p-5 sm:p-8 md:col-span-2', flip && 'md:order-last')}>
        <p className="max-w-sm text-[17px] leading-[1.4] font-medium text-pretty sm:text-lg">
          {title} <span className="text-ds-muted">{children}</span>
        </p>
        {href && (
          <Link
            href={href}
            className="group mt-5 inline-flex w-fit items-center gap-1.5 text-sm font-medium text-ds-accent outline-none focus-visible:underline"
          >
            Learn more
            <ArrowRight aria-hidden className="size-4 transition-transform duration-200 group-hover:translate-x-0.5" />
          </Link>
        )}
      </div>
      <Wallpaper scene={scene} className="flex min-h-[340px] items-center justify-center p-3 sm:min-h-[420px] sm:p-10 md:col-span-3">
        {bare ? demo : <div className="flex w-full max-w-md justify-center rounded-xl bg-white p-4 sm:p-6">{demo}</div>}
      </Wallpaper>
    </Reveal>
  );
}
