"use client";

import { useEffect, useState } from "react";

/** True on devices that can be touched, whatever their pointer media queries claim. */
export function useTouchCapable() {
  const [canTouch, setCanTouch] = useState(false);

  useEffect(() => {
    if (typeof window === "undefined") return;
    const mq = window.matchMedia?.("(any-pointer: coarse)");
    const update = () => setCanTouch(Boolean(mq?.matches) || navigator.maxTouchPoints > 0);
    update();
    mq?.addEventListener?.("change", update);
    return () => mq?.removeEventListener?.("change", update);
  }, []);

  return canTouch;
}
