import { Hono } from "hono";
import type { AppEnv } from "../env";
import { fail } from "../errors";
import { issueLicense } from "../issue";
import { readObject, str } from "../validate";

/** Admin actions, for `bun run license:create`. Off unless ADMIN_TOKEN is set
 * (a secret on the Worker); every call must carry it as a bearer token. */
export const admin = new Hono<AppEnv>();

admin.use("*", async (c, next) => {
  const token = c.env.ADMIN_TOKEN;
  if (!token) return fail(c, 404, "not_found", "Not found.");
  const given = (c.req.header("authorization") ?? "").replace(/^Bearer\s+/i, "");
  if (!(await sameSecret(given, token))) return fail(c, 401, "unauthorized", "Not allowed.");
  await next();
});

/** Gives someone a license: issued and emailed like a purchase, with no payment. */
admin.post("/licenses", async (c) => {
  const body = await readObject(c);
  const email = str(body?.email, 254);
  const name = str(body?.name, 100);
  if (!email || !/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email)) return fail(c, 400, "invalid_request", "A valid email is required.");
  const issued = await issueLicense(c.env, { email, name, source: "manual", paymentId: null });
  return c.json({ license_id: issued.license.id, key: issued.key, key_hint: issued.license.key_hint }, 201);
});

/** Compares two secrets in constant time, by comparing their SHA-256 digests
 * (always 32 bytes), so neither the length nor the content leaks through timing. */
async function sameSecret(a: string, b: string): Promise<boolean> {
  const digest = async (s: string) => new Uint8Array(await crypto.subtle.digest("SHA-256", new TextEncoder().encode(s)));
  const [x, y] = await Promise.all([digest(a), digest(b)]);
  let diff = 0;
  for (let i = 0; i < x.length; i++) diff |= x[i] ^ y[i];
  return diff === 0;
}
