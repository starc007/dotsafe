import { useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { motion, useReducedMotion } from "motion/react";
import { base } from "@/lib/motion";

/** Follows its content's height with a short ease instead of jumping, e.g. a
 * dialog growing when a pasted file adds a preview. The first height lands
 * without animating, so opening isn't slowed down. */
export function AutoHeight({ children, className }: { children: ReactNode; className?: string }) {
  const inner = useRef<HTMLDivElement>(null);
  const [height, setHeight] = useState<number | "auto">("auto");
  // Reduce Motion in MotionConfig covers transforms, not height.
  const reduce = useReducedMotion();

  useLayoutEffect(() => {
    const el = inner.current;
    if (!el) return;
    const observer = new ResizeObserver(() => setHeight(el.offsetHeight));
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  return (
    <motion.div
      initial={false}
      animate={{ height }}
      transition={reduce || height === "auto" ? { duration: 0 } : base}
      // Hides content past the animated edge while it moves. Dialog content is
      // padded, so focus rings stay clear of the edges.
      style={{ overflow: "clip" }}
      className={className}
    >
      <div ref={inner}>{children}</div>
    </motion.div>
  );
}
