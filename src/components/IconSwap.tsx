import type { ReactNode } from "react";
import { AnimatePresence, motion } from "motion/react";
import { fast } from "@/lib/motion";

/** Swaps between two icons with a quick scale and fade, e.g. Copy → Check. */
export function IconSwap({ active, on, off }: { active: boolean; on: ReactNode; off: ReactNode }) {
  return (
    <span className="relative inline-flex size-4 items-center justify-center">
      <AnimatePresence initial={false} mode="popLayout">
        <motion.span
          key={active ? "on" : "off"}
          className="inline-flex"
          initial={{ opacity: 0, scale: 0.6 }}
          animate={{ opacity: 1, scale: 1 }}
          exit={{ opacity: 0, scale: 0.6 }}
          transition={fast}
        >
          {active ? on : off}
        </motion.span>
      </AnimatePresence>
    </span>
  );
}
