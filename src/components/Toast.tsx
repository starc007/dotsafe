import { useCallback, useEffect, useRef, useState } from "react";

export function useToast() {
  const [msg, setMsg] = useState<string | null>(null);
  const timer = useRef<number | undefined>(undefined);
  const show = useCallback((text: string) => {
    window.clearTimeout(timer.current);
    setMsg(text);
    timer.current = window.setTimeout(() => setMsg(null), 2500);
  }, []);
  useEffect(() => () => window.clearTimeout(timer.current), []);
  return { message: msg, show };
}

/** A status line at the bottom of the window, announced to screen readers. */
export function Toast({ message }: { message: string | null }) {
  return (
    <div aria-live="polite" className="pointer-events-none fixed inset-x-0 bottom-4 flex justify-center">
      {message && (
        <div className="animate-in rounded-lg bg-primary px-3 py-2 text-[13px] text-primary-foreground shadow-lg">
          {message}
        </div>
      )}
    </div>
  );
}
