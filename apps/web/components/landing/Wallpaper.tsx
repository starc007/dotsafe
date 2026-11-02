'use client';

import { useRef, type ReactNode } from 'react';
import { GrainGradient, MeshGradient, Warp } from '@paper-design/shaders-react';
import { useInView, useReducedMotion } from 'motion/react';
import { cn } from '@/lib/cn';

/** Painted scenes in the brand's greens, each with a CSS gradient of the same
 * colours underneath, so the frame never flashes empty before WebGL starts. */
const SCENES = {
  meadow: {
    fallback: 'linear-gradient(135deg,#f7f3e3,#d9f99d 45%,#7dd3fc)',
    render: (speed: number) => (
      <MeshGradient
        colors={['#f7f3e3', '#d9f99d', '#a3e635', '#7dd3fc', '#4d7c0f']}
        distortion={0.9}
        swirl={0.7}
        grainMixer={0.3}
        grainOverlay={0.3}
        speed={speed * 0.6}
      />
    ),
  },
  grove: {
    fallback: 'linear-gradient(160deg,#1a2e05,#3f6212 50%,#bef264)',
    render: (speed: number) => (
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
  },
  dune: {
    fallback: 'linear-gradient(135deg,#f5efe0,#fde68a 50%,#a3e635)',
    render: (speed: number) => (
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
  },
  tide: {
    fallback: 'linear-gradient(135deg,#e0f2fe,#7dd3fc 45%,#bef264)',
    render: (speed: number) => (
      <MeshGradient
        colors={['#e0f2fe', '#7dd3fc', '#bef264', '#0f766e']}
        distortion={0.8}
        swirl={0.5}
        grainMixer={0.25}
        grainOverlay={0.3}
        speed={speed * 0.6}
      />
    ),
  },
  moss: {
    fallback: 'linear-gradient(135deg,#ecfccb,#84cc16 50%,#1c1917)',
    render: (speed: number) => (
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
  },
} as const;

export type Scene = keyof typeof SCENES;

/** A framed, slowly moving painting for product shots and demos to sit on. It
 * only animates while on screen, and holds still for reduced motion. */
export function Wallpaper({ scene, className, children }: { scene: Scene; className?: string; children?: ReactNode }) {
  const ref = useRef<HTMLDivElement>(null);
  const inView = useInView(ref, { margin: '200px' });
  const reduce = useReducedMotion();
  const { fallback, render } = SCENES[scene];

  return (
    <div ref={ref} className={cn('relative isolate overflow-hidden rounded-2xl', className)} style={{ background: fallback }}>
      <div aria-hidden className="pointer-events-none absolute inset-0 -z-10 [&>*]:size-full">
        {render(inView && !reduce ? 0.35 : 0)}
      </div>
      {children}
    </div>
  );
}
