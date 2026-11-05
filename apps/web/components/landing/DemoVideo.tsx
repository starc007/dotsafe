'use client';

import { useRef, useState } from 'react';
import { Play } from 'lucide-react';
import { track } from '@/lib/analytics';
import { buttonClass } from '@/lib/button';
import { demo } from '@/lib/site';
import { Reveal } from '@/components/landing/Reveal';
import { SectionHeading } from '@/components/landing/SectionHeading';

/** A walkthrough of the app. Nothing loads until it's played: the poster
 * stands in, and native controls take over once it starts. */
export function DemoVideo() {
  const video = useRef<HTMLVideoElement>(null);
  const [playing, setPlaying] = useState(false);

  function play() {
    setPlaying(true);
    void video.current?.play();
    track('demo_played');
  }

  return (
    <section id="demo" className="mx-auto max-w-6xl scroll-mt-20 px-4 pt-20 sm:px-6 sm:pt-28">
      <SectionHeading eyebrow="Demo" title="See it in a minute.">
        Add a variable, switch environments, find any key and lock the vault.
      </SectionHeading>
      <Reveal className="mt-10">
        <div className="relative overflow-hidden rounded-2xl border border-ds-line bg-ds-shell">
          <video
            ref={video}
            poster={demo.poster}
            preload="none"
            playsInline
            controls={playing}
            width={demo.width}
            height={demo.height}
            aria-label="A one-minute walkthrough of the dotsafe app"
            className="block aspect-[1856/1080] h-auto w-full"
          >
            <source src={demo.webm} type="video/webm" />
            <source src={demo.mp4} type="video/mp4" />
          </video>
          {!playing && (
            <button
              type="button"
              onClick={play}
              className="group absolute inset-0 flex items-center justify-center bg-black/10 transition-colors duration-200 hover:bg-black/20 focus-visible:bg-black/20 focus-visible:outline-none"
            >
              <span className={buttonClass('primary', 'pl-4 group-hover:bg-ds-primary/85 [&_svg]:fill-current')}>
                <Play aria-hidden />
                Watch the demo
                <span className="opacity-60">{demo.length}</span>
              </span>
            </button>
          )}
        </div>
      </Reveal>
    </section>
  );
}
