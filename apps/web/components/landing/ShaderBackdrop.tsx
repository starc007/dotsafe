'use client';

import { MeshGradient } from '@paper-design/shaders-react';
import { useReducedMotion } from 'motion/react';
import { SHADER_COLORS } from '@/lib/motion';
import { cn } from '@/lib/cn';

/** A slow WebGL mesh gradient in the brand colours. It holds still for
 * visitors who ask for reduced motion. */
export function ShaderBackdrop({ className, speed = 0.25 }: { className?: string; speed?: number }) {
  const reduce = useReducedMotion();
  return (
    <MeshGradient
      aria-hidden
      className={cn('pointer-events-none absolute inset-0', className)}
      colors={SHADER_COLORS}
      distortion={0.85}
      swirl={0.55}
      grainMixer={0}
      grainOverlay={0.05}
      speed={reduce ? 0 : speed}
    />
  );
}
