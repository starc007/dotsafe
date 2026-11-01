const DAY_MS = 24 * 60 * 60 * 1000;
const relative = new Intl.RelativeTimeFormat(undefined, { numeric: "auto" });
const date = new Intl.DateTimeFormat(undefined, { dateStyle: "medium" });

/** "today", "yesterday", "12 days ago", then a date after a year. */
export function daysAgo(ms: number): string {
  const days = Math.floor((Date.now() - ms) / DAY_MS);
  return days > 365 ? date.format(ms) : relative.format(-days, "day");
}

/** "20 seconds ago", "5 minutes ago", "3 hours ago", then days. */
export function timeAgo(ms: number): string {
  const secs = Math.max(0, Math.floor((Date.now() - ms) / 1000));
  if (secs < 60) return relative.format(-secs, "second");
  if (secs < 3600) return relative.format(-Math.floor(secs / 60), "minute");
  if (secs < 86_400) return relative.format(-Math.floor(secs / 3600), "hour");
  return daysAgo(ms);
}

export function formatDate(ms: number): string {
  return date.format(ms);
}
