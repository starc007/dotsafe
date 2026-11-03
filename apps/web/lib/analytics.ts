import type { EventProperties, TracwellClient } from 'tracwell';

/** The one Tracwell client, set by <TracwellAnalytics /> once the page is in
 * the browser. Undefined during the static build and before that first effect,
 * so `track()` is a no-op there. */
let client: TracwellClient | undefined;

export function setAnalytics(next: TracwellClient) {
  client ??= next;
}

export function getAnalytics() {
  return client;
}

/** Records a completed outcome. Never pass keys, emails or other personal data. */
export function track(event: string, properties?: EventProperties) {
  client?.track(event, properties);
}
