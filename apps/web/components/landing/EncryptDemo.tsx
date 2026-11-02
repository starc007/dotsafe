'use client';

import { useEffect, useRef, useState } from 'react';
import { motion, useInView, useReducedMotion } from 'motion/react';
import { Cloud, Laptop, Lock } from 'lucide-react';

const VALUE = 'sk_live_51HqP2xR8';
const CIPHER = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';
const CIPHER_LENGTH = 22;

function noise() {
  return Array.from({ length: CIPHER_LENGTH }, () => CIPHER[Math.floor(Math.random() * CIPHER.length)]).join('');
}

/** A value on this Mac, the same value as noise in your storage, and back on
 * your other Mac. The noise keeps changing while it's on screen. */
export function EncryptDemo() {
  const ref = useRef<HTMLDivElement>(null);
  const inView = useInView(ref, { margin: '-60px' });
  const reduce = useReducedMotion();
  const [cipher, setCipher] = useState('Xq7Lm2Zp9vRt4KbN8wYs3e');

  useEffect(() => {
    if (!inView || reduce) return;
    const id = setInterval(() => setCipher(noise()), 90);
    return () => clearInterval(id);
  }, [inView, reduce]);

  return (
    <div ref={ref} className="grid w-full items-center gap-3 md:grid-cols-[1fr_auto_1fr_auto_1fr]">
      <Place icon={<Laptop aria-hidden />} name="This Mac" value={VALUE} />
      <Flow reduce={!!reduce} />
      <Place icon={<Cloud aria-hidden />} name="Your storage" value={cipher} noisy />
      <Flow reduce={!!reduce} delay={0.6} />
      <Place icon={<Laptop aria-hidden />} name="Your other Mac" value={VALUE} />
    </div>
  );
}

function Place({ icon, name, value, noisy }: { icon: React.ReactNode; name: string; value: string; noisy?: boolean }) {
  return (
    <div className="rounded-xl border border-ds-line bg-white p-4">
      <p className="flex items-center gap-2 text-xs text-ds-muted [&_svg]:size-4">
        {icon}
        {name}
        {noisy && <Lock aria-hidden className="ml-auto text-ds-primary" />}
      </p>
      <p className={`mt-3 truncate font-mono text-[13px] ${noisy ? 'text-ds-muted' : ''}`}>{value}</p>
    </div>
  );
}

/** A dot travelling along a short line: data moving between places. Across
 * on wide screens, down on narrow ones, where the places stack. */
function Flow({ reduce, delay = 0 }: { reduce: boolean; delay?: number }) {
  const travel = { duration: 1.4, repeat: Infinity, ease: 'easeInOut' as const, delay };
  const dot = 'absolute size-1.5 rounded-full bg-ds-primary';
  return (
    <div aria-hidden className="relative mx-auto h-8 w-px bg-ds-line md:h-px md:w-12">
      {!reduce && (
        <>
          <motion.span
            className={`${dot} -left-[2.5px] md:hidden`}
            animate={{ top: ['0%', '100%'], opacity: [0, 1, 1, 0] }}
            transition={travel}
          />
          <motion.span
            className={`${dot} -top-[2.5px] hidden md:block`}
            animate={{ left: ['0%', '100%'], opacity: [0, 1, 1, 0] }}
            transition={travel}
          />
        </>
      )}
    </div>
  );
}
