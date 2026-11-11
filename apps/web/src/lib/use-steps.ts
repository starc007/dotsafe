import { useEffect, useRef, useState } from 'react';
import { useInView, useReducedMotion } from 'motion/react';

/**
 * Plays a demo's beats while it's on screen: step `i` lasts `durations[i]` ms,
 * then the next; after the last, `round` goes up and it starts again. Step 0
 * is the finished state, which reduced motion shows and holds.
 */
export function useSteps(durations: number[]) {
  const ref = useRef<HTMLDivElement>(null);
  const inView = useInView(ref, { margin: '-60px' });
  const reduce = useReducedMotion();
  const [state, setState] = useState({ step: 0, round: 0 });

  useEffect(() => {
    if (!inView || reduce) return;
    const id = setTimeout(
      () => setState((s) => (s.step + 1 >= durations.length ? { step: 0, round: s.round + 1 } : { ...s, step: s.step + 1 })),
      durations[state.step],
    );
    return () => clearTimeout(id);
  }, [inView, reduce, state, durations]);

  return { ref, step: reduce ? 0 : state.step, round: state.round };
}
