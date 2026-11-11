/** The painted scenes behind product shots and demos, in the brand's greens.
 * Each has a CSS gradient of the same colours, shown before WebGL starts (and
 * instead of it, without JavaScript). The shaders are in
 * src/components/landing/WallpaperPaint.tsx. */
export const SCENE_FALLBACKS = {
  meadow: 'linear-gradient(135deg,#f7f3e3,#d9f99d 45%,#7dd3fc)',
  grove: 'linear-gradient(160deg,#1a2e05,#3f6212 50%,#bef264)',
  dune: 'linear-gradient(135deg,#f5efe0,#fde68a 50%,#a3e635)',
  tide: 'linear-gradient(135deg,#e0f2fe,#7dd3fc 45%,#bef264)',
  moss: 'linear-gradient(135deg,#ecfccb,#84cc16 50%,#1c1917)',
};

export type Scene = keyof typeof SCENE_FALLBACKS;
