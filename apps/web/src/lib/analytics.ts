import type { EventProperties, TracwellClient } from 'tracwell';

/** The one Tracwell client, set by src/scripts/analytics.ts once the page is
 * in the browser. Undefined during the static build and before that script
 * runs, so `track()` is a no-op there. */
let client: TracwellClient | undefined;

export function setAnalytics(next: TracwellClient) {
  client ??= next;
}

/** Records a completed outcome. Never pass keys, emails or other personal data. */
export function track(event: string, properties?: EventProperties) {
  client?.track(event, properties);
}
