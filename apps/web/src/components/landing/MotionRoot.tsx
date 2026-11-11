import type { ReactNode } from 'react';
import { MotionConfig } from 'motion/react';

/** Follows the visitor's reduce-motion setting for every animation below.
 * Each island is its own React root, so each animated one wraps itself in this. */
export function MotionRoot({ children }: { children: ReactNode }) {
  return <MotionConfig reducedMotion="user">{children}</MotionConfig>;
}
