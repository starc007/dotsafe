import type { Context } from "hono";

/** Reads a JSON object body, or null when the body is missing or not an object. */
export async function readObject(c: Context): Promise<Record<string, unknown> | null> {
  try {
    const body = await c.req.json();
    return body && typeof body === "object" && !Array.isArray(body) ? body : null;
  } catch {
    return null;
  }
}

/** A trimmed non-empty string no longer than `max`, or null. */
export function str(value: unknown, max: number): string | null {
  if (typeof value !== "string") return null;
  const s = value.trim();
  return s.length > 0 && s.length <= max ? s : null;
}
