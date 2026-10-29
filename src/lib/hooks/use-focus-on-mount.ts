import { useEffect, useRef } from "react";

/**
 * Focuses the element when it mounts, without scrolling to it.
 *
 * Use it instead of `autoFocus` for fields that appear inside an open dialog.
 * The dialog eases to its new height (`AutoHeight`), and `autoFocus` would
 * scroll the dialog to the field's final spot before it has grown, then the
 * scroll unwinds as it grows: a visible jump.
 */
export function useFocusOnMount<T extends HTMLElement>(enabled = true) {
  const ref = useRef<T>(null);
  useEffect(() => {
    if (enabled) ref.current?.focus({ preventScroll: true });
  }, [enabled]);
  return ref;
}
