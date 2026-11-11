import { useRef } from 'react';
import { GrainGradient, MeshGradient, Warp } from '@paper-design/shaders-react';
import { useInView, useReducedMotion } from 'motion/react';
import { cn } from '@/lib/cn';
import type { Scene } from '@/lib/scenes';
import { useSettled } from '@/lib/use-settled';

const SCENES: Record<Scene, (speed: number) => React.ReactNode> = {
  meadow: (speed) => (
    <MeshGradient
      colors={['#f7f3e3', '#d9f99d', '#a3e635', '#7dd3fc', '#4d7c0f']}
      distortion={0.9}
      swirl={0.7}
      grainMixer={0.3}
      grainOverlay={0.3}
      speed={speed * 0.6}
    />
  ),
  grove: (speed) => (
    <Warp
      colors={['#1a2e05', '#3f6212', '#84cc16', '#ecfccb']}
      proportion={0.42}
      softness={1}
      distortion={0.22}
      swirl={0.85}
      swirlIterations={10}
      shape="edge"
      shapeScale={0.12}
      scale={1.6}
      speed={speed}
    />
  ),
  dune: (speed) => (
    <GrainGradient
      colorBack="#f5efe0"
      colors={['#fde68a', '#bef264', '#65a30d', '#fdba74']}
      softness={0.7}
      intensity={0.25}
      noise={0.3}
      shape="blob"
      speed={speed}
    />
  ),
  tide: (speed) => (
    <MeshGradient
      colors={['#e0f2fe', '#7dd3fc', '#bef264', '#0f766e']}
      distortion={0.8}
      swirl={0.5}
      grainMixer={0.25}
      grainOverlay={0.3}
      speed={speed * 0.6}
    />
  ),
  moss: (speed) => (
    <Warp
      colors={['#ecfccb', '#a3e635', '#3f6212', '#1c1917']}
      proportion={0.5}
      softness={1}
      distortion={0.15}
      swirl={0.6}
      swirlIterations={8}
      shape="stripes"
      shapeScale={0.2}
      rotation={-30}
      speed={speed}
    />
  ),
};

/** The moving paint inside a Wallpaper (Wallpaper.astro), as its own island.
 * Starting a shader is heavy, so it waits until the page has loaded and gone
 * idle, and until the frame first comes near the screen, then fades in over
 * the frame's CSS gradient. It only animates while on screen, and holds still
 * for reduced motion. */
export function WallpaperPaint({ scene }: { scene: Scene }) {
  const ref = useRef<HTMLDivElement>(null);
  const inView = useInView(ref, { margin: '200px' });
  const seen = useInView(ref, { margin: '200px', once: true });
  const reduce = useReducedMotion();
  const settled = useSettled();
  const started = settled && seen;
  return (
    <div
      ref={ref}
      aria-hidden
      className={cn(
        'pointer-events-none absolute inset-0 -z-10 transition-opacity duration-700 ease-[cubic-bezier(0.16,1,0.3,1)] motion-reduce:transition-none [&>*]:size-full',
        !started && 'opacity-0',
      )}
    >
      {started && SCENES[scene](inView && !reduce ? 0.35 : 0)}
    </div>
  );
}
