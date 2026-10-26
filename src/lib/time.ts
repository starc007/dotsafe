const DAY_MS = 24 * 60 * 60 * 1000;
const relative = new Intl.RelativeTimeFormat(undefined, { numeric: "auto" });
const date = new Intl.DateTimeFormat(undefined, { dateStyle: "medium" });

/** "today", "yesterday", "12 days ago", then a date after a year. */
export function daysAgo(ms: number): string {
  const days = Math.floor((Date.now() - ms) / DAY_MS);
  return days > 365 ? date.format(ms) : relative.format(-days, "day");
}

export function formatDate(ms: number): string {
  return date.format(ms);
}
