import { useEffect, useRef, useState } from 'react';
import { motion, useInView, useReducedMotion } from 'motion/react';
import { EASE } from '@/lib/motion';
import { MotionRoot } from '@/components/landing/MotionRoot';

const SESSION = [
  { cmd: 'dotsafe run -- bun dev', out: 'my-api/development · 12 variables' },
  { cmd: 'dotsafe scan', out: 'src/config.ts:14  STRIPE_KEY' },
  { cmd: 'dotsafe import --from doppler', out: 'Imported 18 variables' },
];
const TYPE_MS = 38;
const AFTER_OUTPUT_MS = 900;
const AFTER_SESSION_MS = 1800;

/** A terminal session typing itself out, one command at a time. */
export function TerminalDemo() {
  return (
    <MotionRoot>
      <Demo />
    </MotionRoot>
  );
}

function Demo() {
  const ref = useRef<HTMLDivElement>(null);
  const inView = useInView(ref, { margin: '-60px' });
  const reduce = useReducedMotion();
  // `line` commands are done; `chars` of the next one are typed.
  const [pos, setPos] = useState({ line: 0, chars: 0 });

  useEffect(() => {
    if (!inView || reduce) return;
    const current = SESSION[pos.line];
    let next: typeof pos;
    let wait = TYPE_MS;
    if (!current) {
      next = { line: 0, chars: 0 };
      wait = AFTER_SESSION_MS;
    } else if (pos.chars < current.cmd.length) {
      next = { ...pos, chars: pos.chars + 1 };
    } else {
      next = { line: pos.line + 1, chars: 0 };
      wait = AFTER_OUTPUT_MS;
    }
    const id = setTimeout(() => setPos(next), wait);
    return () => clearTimeout(id);
  }, [inView, reduce, pos]);

  const shown = reduce ? { line: SESSION.length, chars: 0 } : pos;
  return (
    <div ref={ref} className="w-full max-w-md overflow-hidden rounded-xl border border-ds-line bg-white">
      <div className="flex items-center gap-1.5 border-b border-ds-line px-3.5 py-2.5">
        {[0, 1, 2].map((i) => (
          <span key={i} className="size-2.5 rounded-full border border-ds-line" />
        ))}
        <span className="ml-2 text-xs text-ds-muted">~/my-api</span>
      </div>
      <div className="h-[184px] p-4 font-mono text-[12.5px] leading-6">
        {SESSION.map((entry, i) => {
          if (i > shown.line) return null;
          const typing = i === shown.line;
          return (
            <div key={entry.cmd} className={i ? 'mt-1.5' : undefined}>
              <p className="whitespace-pre">
                <span className="text-ds-accent select-none">$ </span>
                {typing ? entry.cmd.slice(0, shown.chars) : entry.cmd}
                {typing && <span className="ml-px inline-block h-4 w-[7px] translate-y-[3px] animate-pulse bg-ds-fg/70" />}
              </p>
              {!typing && (
                <motion.p
                  initial={{ opacity: 0 }}
                  animate={{ opacity: 1 }}
                  transition={{ duration: 0.3, ease: EASE }}
                  className="truncate text-ds-muted"
                >
                  {entry.out}
                </motion.p>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
