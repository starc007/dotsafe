/** Shared motion: one ease-out curve, short durations, nothing bouncy. */
export const EASE = [0.16, 1, 0.3, 1] as const;

/** Fade up into place, for content entering the page. */
export const fadeUp = {
  hidden: { opacity: 0, y: 16 },
  shown: { opacity: 1, y: 0, transition: { duration: 0.7, ease: EASE } },
};