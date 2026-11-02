'use client';

import { useRef } from 'react';
import Image from 'next/image';
import { motion, useReducedMotion, useScroll, useTransform } from 'motion/react';
import { EASE } from '@/lib/motion';

/** The real app, rising over the gradient and settling flat as it scrolls in. */
export function ProductShot() {
  const ref = useRef<HTMLDivElement>(null);
  const reduce = useReducedMotion();
  const { scrollYProgress } = useScroll({ target: ref, offset: ['start end', 'center center'] });
  const scale = useTransform(scrollYProgress, [0, 1], reduce ? [1, 1] : [0.94, 1]);
  const rotateX = useTransform(scrollYProgress, [0, 1], reduce ? [0, 0] : [10, 0]);

  return (
    <div ref={ref} className="relative mx-auto mt-12 max-w-6xl px-4 [perspective:1600px] sm:mt-14 sm:px-6">
      <motion.div
        initial={{ opacity: 0, y: 40 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.9, ease: EASE, delay: 0.35 }}
        style={{ scale, rotateX }}
        className="origin-top rounded-2xl border border-ds-line bg-white/70 p-1.5 backdrop-blur-sm sm:p-2"
      >
        <Image
          src="/app.png"
          alt="The dotsafe app: a project's variables, with a secret linked across environments"
          width={2080}
          height={1120}
          priority
          className="w-full rounded-xl border border-ds-line"
        />
      </motion.div>
    </div>
  );
}
