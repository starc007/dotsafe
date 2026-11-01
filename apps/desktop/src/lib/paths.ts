const ICLOUD = /^.*\/Library\/Mobile Documents\/com~apple~CloudDocs/;

/** A vault path as a person would recognize it: iCloud Drive by name, and
 * long local paths shortened to their last two folders. */
export function displayPath(path: string): string {
  if (path.startsWith("s3://")) return path;
  if (ICLOUD.test(path)) return path.replace(ICLOUD, "iCloud Drive");
  const home = path.replace(/^\/Users\/[^/]+/, "~");
  const parts = home.split("/").filter(Boolean);
  return parts.length > 3 ? `…/${parts.slice(-2).join("/")}` : home;
}
