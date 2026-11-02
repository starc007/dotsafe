import Image from 'next/image';
import { RELEASES } from '@/lib/changelog';
import { pageMetadata } from '@/lib/seo';
import { Reveal } from '@/components/landing/Reveal';
import { Wallpaper } from '@/components/landing/Wallpaper';

export const metadata = pageMetadata({
  title: 'Changelog',
  description: 'New features, fixes and improvements in each dotsafe release.',
  path: '/changelog',
});

const dateFormat = new Intl.DateTimeFormat('en', { month: 'short', day: 'numeric', year: 'numeric', timeZone: 'UTC' });

/** Releases as a timeline held together by space, not lines: the date and
 * version stay pinned on the left while the release scrolls past. */
export default function ChangelogPage() {
  return (
    <section className="mx-auto max-w-6xl px-4 pt-32 pb-28 sm:px-6 sm:pt-40">
      <h1 className="max-w-2xl text-[28px] leading-[1.2] font-medium tracking-[-0.02em] text-balance sm:text-[36px]">
        Changelog <span className="text-ds-muted">New features, fixes and improvements in each release.</span>
      </h1>
      <ol className="mt-16 flex flex-col gap-24 sm:mt-24 sm:gap-32">
        {RELEASES.map((release) => (
          <li key={release.version} className="grid gap-5 md:grid-cols-4 md:gap-10">
            <div className="flex items-baseline gap-3 self-start md:sticky md:top-28 md:flex-col md:gap-1">
              <p className="text-sm font-medium">
                {release.date ? (
                  <time dateTime={release.date}>{dateFormat.format(new Date(release.date))}</time>
                ) : (
                  'Upcoming'
                )}
              </p>
              <p className="font-mono text-xs text-ds-muted tabular-nums">v{release.version}</p>
            </div>
            <Reveal className="md:col-span-3">
              <h2 className="text-2xl leading-tight font-medium tracking-[-0.015em] text-balance sm:text-[28px]">
                {release.title}
              </h2>
              {release.summary && (
                <p className="mt-3 max-w-xl text-[15px] leading-relaxed text-pretty text-ds-muted">{release.summary}</p>
              )}
              {release.image && (
                <Wallpaper scene={release.scene ?? 'meadow'} className="mt-8 px-4 pt-8 sm:px-10 sm:pt-12">
                  <div className="rounded-t-xl bg-white/40 p-1.5 pb-0 backdrop-blur-md">
                    <Image
                      src={release.image.src}
                      alt={release.image.alt}
                      width={release.image.width}
                      height={release.image.height}
                      className="w-full rounded-t-lg"
                    />
                  </div>
                </Wallpaper>
              )}
              <ul className="mt-8 grid max-w-2xl gap-x-10 gap-y-4 text-sm leading-relaxed sm:grid-cols-2">
                {release.changes.map((change) => (
                  <li key={change} className="flex gap-3 text-pretty">
                    <span aria-hidden className="mt-[8px] size-1.5 shrink-0 rounded-full bg-ds-primary" />
                    {change}
                  </li>
                ))}
              </ul>
            </Reveal>
          </li>
        ))}
      </ol>
    </section>
  );
}
