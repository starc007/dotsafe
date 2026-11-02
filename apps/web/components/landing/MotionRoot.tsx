'use client';

import type { ReactNode } from 'react';
import { MotionConfig } from 'motion/react';

/** Follows the visitor's reduce-motion setting for every animation below. */
export function MotionRoot({ children }: { children: ReactNode }) {
  return <MotionConfig reducedMotion="user">{children}</MotionConfig>;
}
