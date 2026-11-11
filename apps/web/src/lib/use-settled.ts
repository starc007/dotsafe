import { useEffect, useState } from 'react';

/** Resolves once the page has loaded and the browser has a free moment, so
 * decoration (the wallpaper shaders) never competes with the page itself. */
const settled = new Promise<void>((resolve) => {
  if (typeof window === 'undefined') return;
  const idle = () =>
    'requestIdleCallback' in window ? window.requestIdleCallback(() => resolve(), { timeout: 3000 }) : setTimeout(resolve, 200);
  if (document.readyState === 'complete') idle();
  else window.addEventListener('load', idle, { once: true });
});

/** False until the page has settled (see above), then true for good. */
export function useSettled() {
  const [done, setDone] = useState(false);
  useEffect(() => {
    let live = true;
    void settled.then(() => live && setDone(true));
    return () => {
      live = false;
    };
  }, []);
  return done;
}
