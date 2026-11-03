// License keys: made, normalized and hashed here, and nowhere else. A key is
// "DOTSAFE-" and four groups of five characters from a 32-letter alphabet
// without look-alikes (no I, L, O or U): 100 random bits, far past guessing.
// Only its SHA-256 hash is stored.

const ALPHABET = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/** A new random license key. */
export function newKey(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(20));
  // 256 is a multiple of 32, so taking each byte mod 32 stays uniform.
  const chars = Array.from(bytes, (b) => ALPHABET[b % 32]).join("");
  return `DOTSAFE-${chars.slice(0, 5)}-${chars.slice(5, 10)}-${chars.slice(10, 15)}-${chars.slice(15, 20)}`;
}

/** A random id with a prefix, like "lic_…" or "act_…". */
export function newId(prefix: string): string {
  const bytes = crypto.getRandomValues(new Uint8Array(16));
  return `${prefix}_${Array.from(bytes, (b) => ALPHABET[b % 32]).join("").toLowerCase()}`;
}

/** The form a key is hashed in: trimmed and upper-case, so a pasted key in
 * lower case still matches. Look-alike letters aren't guessed at. */
export function normalizeKey(key: string): string {
  return key.trim().toUpperCase();
}

/** The SHA-256 of the normalized key, as hex: what the database keeps. */
export async function hashKey(key: string): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(normalizeKey(key)));
  return Array.from(new Uint8Array(digest), (b) => b.toString(16).padStart(2, "0")).join("");
}

/** The last four characters, for support ("the key ending in 7KQ2"). */
export function keyHint(key: string): string {
  return normalizeKey(key).slice(-4);
}
