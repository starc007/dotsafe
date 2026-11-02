import { useEffect, useState } from "react";
import { Dithering } from "@paper-design/shaders-react";
import { motion, useReducedMotion } from "motion/react";
import { base } from "@/lib/motion";
import { useTheme } from "@/lib/theme";

function tokens() {
  const style = getComputedStyle(document.documentElement);
  return {
    back: style.getPropertyValue("--backdrop-back").trim() || "#ffffff",
    front: style.getPropertyValue("--backdrop-front").trim() || "#d9f99d",
  };
}

/** A slow, softly blurred dithered field behind the pre-vault screens, in the theme's
 * --backdrop-* colours. App mounts it once for the whole flow, so it keeps
 * running from screen to screen. It holds still for reduced motion. */
export function OnboardingBackdrop() {
  const { theme } = useTheme();
  const reduce = useReducedMotion();
  const [colors, setColors] = useState(tokens);

  // The tokens change with the theme class on <html>; read them again after it flips.
  useEffect(() => setColors(tokens()), [theme]);

  return (
    <motion.div
      aria-hidden
      className="pointer-events-none fixed inset-0 bg-background will-change-[opacity]"
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      transition={base}
    >
      <Dithering
        className="size-full blur-[2px]"
        colorBack={colors.back}
        colorFront={colors.front}
        shape="warp"
        type="4x4"
        size={2}
        scale={0.8}
        speed={reduce ? 0 : 0.25}
      />
      {/* A wash of the background over the softened dots, so the field stays
          behind the card instead of competing with it. */}
      <div className="absolute inset-0 bg-background/45" />
    </motion.div>
  );
}
