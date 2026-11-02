'use client';

import { AnimatePresence, motion } from 'motion/react';
import { Bot, Check, Fingerprint } from 'lucide-react';
import { EASE } from '@/lib/motion';
import { useSteps } from '@/lib/use-steps';
import { cn } from '@/lib/cn';

// 0: done (approved, output in); 1: a new request waits; 2: Allow, Touch ID; 3: approved; 4 and 5: the output arrives.
const DURATIONS = [2200, 1100, 700, 500, 500, 600];

const appear = {
  initial: { opacity: 0, y: 6 },
  animate: { opacity: 1, y: 0 },
  exit: { opacity: 0 },
  transition: { duration: 0.35, ease: EASE },
};

/** An agent asks to run a command with secrets; the output comes back redacted. */
export function AgentDemo() {
  const { ref, step } = useSteps(DURATIONS);
  const approved = step === 0 || step >= 3;
  const lines = step === 0 ? 2 : Math.max(0, step - 3);

  return (
    <div ref={ref} className="w-full max-w-sm text-sm">
      <div className="flex items-center gap-2.5">
        <span className="flex size-8 shrink-0 items-center justify-center rounded-full border border-ds-line">
          <Bot aria-hidden className="size-4" />
        </span>
        <p className="min-w-0 flex-1 truncate">
          <span className="font-medium">Claude Code</span> <span className="text-ds-muted">wants to run</span>
        </p>
      </div>
      <p className="mt-3 rounded-lg border border-ds-line px-3 py-2 font-mono text-[12.5px]">bun test</p>
      <div className="mt-3 flex flex-wrap gap-1.5">
        {['STRIPE_KEY', 'DATABASE_URL'].map((v) => (
          <span key={v} className="rounded-full border border-ds-line px-2 py-0.5 font-mono text-[11px] text-ds-muted">
            {v}
          </span>
        ))}
      </div>
      <div className="mt-4 flex h-8 items-center">
        <AnimatePresence mode="wait" initial={false}>
          {approved ? (
            <motion.p key="ok" {...appear} className="flex items-center gap-1.5 text-xs font-medium text-ds-accent">
              <Fingerprint aria-hidden className="size-4" />
              Approved with Touch ID
            </motion.p>
          ) : (
            <motion.div key="ask" {...appear} className="flex gap-2">
              <span className="inline-flex h-8 items-center rounded-full px-3 text-xs text-ds-muted">Deny</span>
              <motion.span
                animate={{ scale: step === 2 ? 0.95 : 1 }}
                transition={{ duration: 0.15, ease: EASE }}
                className={cn(
                  'inline-flex h-8 items-center gap-1.5 rounded-full px-3 text-xs font-medium transition-colors',
                  step === 2 ? 'bg-ds-primary text-ds-primary-fg' : 'border border-ds-line',
                )}
              >
                <Fingerprint aria-hidden className="size-3.5" />
                Allow
              </motion.span>
            </motion.div>
          )}
        </AnimatePresence>
      </div>
      <div className="mt-3 min-h-[52px] border-t border-ds-line pt-3 font-mono text-[12px] leading-6">
        <AnimatePresence initial={false}>
          {lines >= 1 && (
            <motion.p key="l1" {...appear} className="flex items-center gap-1.5 text-ds-muted">
              <Check aria-hidden className="size-3.5 text-ds-accent" />
              42 tests passed
            </motion.p>
          )}
          {lines >= 2 && (
            <motion.p key="l2" {...appear} className="truncate text-ds-muted">
              charged card with <span className="rounded bg-ds-primary/30 px-1 text-ds-accent">[dotsafe:STRIPE_KEY]</span>
            </motion.p>
          )}
        </AnimatePresence>
      </div>
    </div>
  );
}
