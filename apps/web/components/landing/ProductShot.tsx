'use client';

import Image from 'next/image';
import { motion } from 'motion/react';
import { EASE } from '@/lib/motion';
import { Wallpaper } from '@/components/landing/Wallpaper';

/** The real app, set on a painted wallpaper like a window on a desktop. */
export function ProductShot() {
  return (
    <motion.div
      className="mx-auto mt-14 max-w-6xl px-4 sm:mt-16 sm:px-6"
      initial={{ opacity: 0, y: 24 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.9, ease: EASE, delay: 0.25 }}
    >
      <Wallpaper scene="meadow" className="px-4 pt-10 sm:px-12 sm:pt-16 md:px-20 md:pt-20">
        <div className="rounded-t-xl bg-white/40 p-1.5 pb-0 backdrop-blur-md sm:p-2 sm:pb-0">
          <Image
            src="/app.png"
            alt="The dotsafe app: a project's variables, with a secret linked across environments"
            width={2080}
            height={1120}
            priority
            className="w-full rounded-t-lg"
          />
        </div>
      </Wallpaper>
    </motion.div>
  );
}
