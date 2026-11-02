'use client';

import { AnimatePresence, motion } from 'motion/react';
import { Check, KeyRound, LoaderCircle, RotateCw } from 'lucide-react';
import { EASE } from '@/lib/motion';
import { useSteps } from '@/lib/use-steps';
import { cn } from '@/lib/cn';

const ENVS = ['development', 'staging', 'production'];
const ENDINGS = ['4f2a', '91c7', '2be0', 'd35f', '70aa', 'c18e', '5b03', 'e96d'];
// 0: done; 1: Rotate pressed, a new version made; 2 to 4: each environment picks it up.
const DURATIONS = [1800, 700, 450, 450, 900];

/** One secret linked into three environments: Rotate, and each one updates. */
export function RotationDemo() {
  const { ref, step, round } = useSteps(DURATIONS);
  const rotating = step >= 1;
  const version = 4 + round + (rotating ? 1 : 0);
  const ending = (v: number) => ENDINGS[v % ENDINGS.length];

  return (
    <div ref={ref} className="w-full max-w-md text-sm">
      <div className="flex items-center gap-3">
        <span className="flex size-9 shrink-0 items-center justify-center rounded-full border border-ds-line">
          <KeyRound aria-hidden className="size-4" />
        </span>
        <div className="min-w-0 flex-1">
          <p className="truncate font-mono text-[13px] font-medium">STRIPE_SECRET_KEY</p>
          <p className="text-xs text-ds-muted">
            <Swap value={`v${version}`} /> · linked in 3 places
          </p>
        </div>
        <motion.span
          animate={{ scale: step === 1 ? 0.95 : 1 }}
          transition={{ duration: 0.15, ease: EASE }}
          className={cn(
            'inline-flex h-8 items-center gap-1.5 rounded-full border px-3 text-xs font-medium transition-colors',
            step === 1 ? 'border-ds-primary bg-ds-primary text-ds-primary-fg' : 'border-ds-line',
          )}
        >
          <RotateCw aria-hidden className={cn('size-3.5', step === 1 && 'animate-spin')} />
          Rotate
        </motion.span>
      </div>
      <ul className="mt-5 divide-y divide-ds-line rounded-xl border border-ds-line">
        {ENVS.map((env, i) => {
          // Before its turn, a row still shows the old value.
          const updated = !rotating || step >= 2 + i;
          const shown = updated ? version : version - 1;
          return (
            <li key={env} className="flex items-center gap-3 px-3.5 py-2.5">
              <span className="w-24 shrink-0 text-ds-muted">{env}</span>
              <span className="flex-1 font-mono text-[12.5px]">
                sk_live_••••
                <Swap value={ending(shown)} />
              </span>
              <span className="flex size-5 items-center justify-center">
                {updated ? (
                  <motion.span key={`${round}-${shown}`} initial={{ scale: 0.6, opacity: 0 }} animate={{ scale: 1, opacity: 1 }} transition={{ duration: 0.3, ease: EASE }}>
                    <Check aria-hidden className="size-4 text-ds-accent" />
                  </motion.span>
                ) : (
                  <LoaderCircle aria-hidden className="size-4 animate-spin text-ds-muted" />
                )}
              </span>
            </li>
          );
        })}
      </ul>
    </div>
  );
}

/** Text that slides up when it changes. */
function Swap({ value }: { value: string }) {
  return (
    <span className="relative inline-flex overflow-hidden align-bottom">
      <AnimatePresence mode="popLayout" initial={false}>
        <motion.span
          key={value}
          initial={{ y: '100%', opacity: 0 }}
          animate={{ y: 0, opacity: 1 }}
          exit={{ y: '-100%', opacity: 0 }}
          transition={{ duration: 0.35, ease: EASE }}
        >
          {value}
        </motion.span>
      </AnimatePresence>
    </span>
  );
}
