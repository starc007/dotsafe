'use client';

import { useEffect } from 'react';
import { createTracwell } from 'tracwell';
import { getAnalytics, setAnalytics, track } from '@/lib/analytics';
import { isDownloadLink } from '@/lib/downloads';
import { tracwell } from '@/lib/site';

/** Query parameters that must never reach analytics: Dodo's return URL to
 * /thanks carries the license key and the buyer's email. */
const PRIVATE_PARAMS = ['license_key', 'email'];

/** Only production builds send anything: `bun run web` (development) doesn't. */
const ENABLED = process.env.NODE_ENV === 'production';

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
    if (!ENABLED || getAnalytics() || PRIVATE_PARAMS.some((p) => params.has(p))) return;
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
      if (link && isDownloadLink(link.href)) track('download_link_opened', { path: window.location.pathname, file: link.href.split('/').pop() ?? '' });
    };
    document.addEventListener('click', onClick);
    return () => document.removeEventListener('click', onClick);
  }, []);

  return null;
}
