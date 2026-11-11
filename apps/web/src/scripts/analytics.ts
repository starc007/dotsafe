import { createTracwell } from 'tracwell';
import { setAnalytics, track } from '@/lib/analytics';
import { isDownloadLink } from '@/lib/downloads';
import { tracwell } from '@/lib/site';

/** Starts Tracwell once per page load, in the browser. The SDK records the
 * page view itself. Only production builds send anything: `bun run web`
 * (development) doesn't.
 *
 * /thanks wipes its query in an inline script before this module runs (module
 * scripts wait for the page to parse). If a private parameter is somehow still
 * in the URL, analytics stays off for this load rather than record it. */

/** Query parameters that must never reach analytics: Dodo's return URL to
 * /thanks carries the license key and the buyer's email. */
const PRIVATE_PARAMS = ['license_key', 'email'];

const params = new URLSearchParams(window.location.search);
if (import.meta.env.PROD && !PRIVATE_PARAMS.some((p) => params.has(p))) {
  setAnalytics(
    createTracwell({
      projectKey: tracwell.projectKey,
      collectionMode: 'product',
      consent: 'granted',
      respectDoNotTrack: true,
    }),
  );
}

// Downloads leave for GitHub, so the click is the last thing the site sees.
// One listener covers every download link on the site.
document.addEventListener('click', (e) => {
  const link = (e.target as Element | null)?.closest?.('a');
  if (link && isDownloadLink(link.href)) {
    track('download_link_opened', { path: window.location.pathname, file: link.href.split('/').pop() ?? '' });
  }
});
