import type { Transition } from "motion/react";

// Motion tokens, so every micro-interaction shares one feel: quick, eased-out,
// never bouncy. MotionConfig in App.tsx turns transform/layout animation off
// for people who enable Reduce Motion.

/** Ease-out (quint-like): fast start, soft landing. */
export const easeOut = [0.22, 1, 0.36, 1] as const;

/** Icon swaps, crossfades. */
export const fast: Transition = { duration: 0.15, ease: easeOut };

/** Rows appearing and leaving, screen entrances. */
export const base: Transition = { duration: 0.22, ease: easeOut };

/** Things that follow the pointer or selection, like the sidebar highlight. */
export const snappy: Transition = { type: "spring", stiffness: 520, damping: 42, mass: 0.8 };
