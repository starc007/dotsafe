'use client';

import { useEffect } from 'react';
import { createTracwell } from 'tracwell';
import { getAnalytics, setAnalytics, track } from '@/lib/analytics';
import { links, tracwell } from '@/lib/site';

/** Query parameters that must never reach analytics: Dodo's return URL to
 * /thanks carries the license key and the buyer's email. */
const PRIVATE_PARAMS = ['license_key', 'email'];

/** Local development and previews (`bun run web`, `wrangler dev`) send nothing. */
function isLocal(hostname: string) {
  return hostname === 'localhost' || hostname === '127.0.0.1' || hostname === '[::1]' || hostname.endsWith('.localhost');
}

/** Starts Tracwell once, in the browser. The SDK records the first page view
 * and follows Next.js navigation through the History API by itself, so no
 * route hook adds page views here.
 *
 * Rendered after the page in the root layout, so its effect runs after the
 * page's own: /thanks has already wiped its query by then. If a private
 * parameter is somehow still in the URL, analytics stays off for this load
 * rather than record it. */
export function TracwellAnalytics() {
  useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    if (getAnalytics() || isLocal(window.location.hostname) || PRIVATE_PARAMS.some((p) => params.has(p))) return;
    setAnalytics(
      createTracwell({
        projectKey: tracwell.projectKey,
        collectionMode: 'product',
        consent: 'granted',
        respectDoNotTrack: true,
      }),
    );
  }, []);

  useEffect(() => {
    // Downloads leave for GitHub, so the click is the last thing the site
    // sees. One listener covers every download link on the site.
    const onClick = (e: MouseEvent) => {
      const link = (e.target as Element | null)?.closest?.('a');
      if (link?.href === links.download) track('download_link_opened', { path: window.location.pathname });
    };
    document.addEventListener('click', onClick);
    return () => document.removeEventListener('click', onClick);
  }, []);

  return null;
}
