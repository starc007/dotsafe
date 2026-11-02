'use client';

import { motion } from 'motion/react';
import { EASE } from '@/lib/motion';

/** A joke YC batch badge: "Backed by Y Combinator", with a small "not" written in
 * above it a beat later, like a proofreader's correction. dotsafe is bootstrapped. */
export function NotYcBadge() {
  return (
    <p
      aria-label="Not backed by Y Combinator. Bootstrapped, 2026."
      className="inline-flex items-center gap-2 rounded-full border border-ds-line py-1 pr-3 pl-1 text-[13px]"
    >
      <span aria-hidden className="flex size-5 items-center justify-center rounded-full bg-ds-yc text-[11px] font-semibold text-white">
        Y
      </span>
      <span aria-hidden className="relative">
        <motion.span
          // The page colour behind it breaks the pill's border, like a note written over it.
          className="absolute -top-3.5 -left-2.5 origin-bottom-left rounded-sm bg-ds-bg px-1 leading-4 font-medium text-ds-yc italic"
          initial={{ opacity: 0, scale: 0.6, rotate: -14 }}
          animate={{ opacity: 1, scale: 1, rotate: -8 }}
          transition={{ delay: 0.9, duration: 0.45, ease: EASE }}
        >
          <span className="text-xs">not</span>
        </motion.span>
        Backed by Y Combinator
      </span>
      <span aria-hidden className="text-ds-muted">Bootstrapped &apos;26</span>
    </p>
  );
}
