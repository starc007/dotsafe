"use client";

import { useIsPresent } from "motion/react";
import type { ReactNode } from "react";

export interface PresenceGateRenderProps {
  /** False from the render that starts the exit animation onward. */
  isPresent: boolean;
  /** Spread onto every layer that takes pointer events while the overlay is
   * open, so interaction releases as soon as the exit starts. */
  gate: {
    inert: boolean;
    style: { pointerEvents: "auto" | "none" };
  };
}

/** Reads the presence of the subtree it renders and hands it down. */
export function PresenceGate({ children }: { children: (props: PresenceGateRenderProps) => ReactNode }) {
  const isPresent = useIsPresent();
  return children({
    isPresent,
    gate: { inert: !isPresent, style: { pointerEvents: isPresent ? "auto" : "none" } },
  });
}
