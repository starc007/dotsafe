import type { MiddlewareHandler } from "hono";
import { bodyLimit } from "hono/body-limit";
import type { AppEnv, Env } from "./env";
import { fail } from "./errors";

// Abuse limits. The code is public, so anyone can script these routes; each
// call spends our Dodo API key, and Dodo rate-limiting that key would lock out
// real buyers and users. Limits are per client IP and per Cloudflare location
// (approximate by design), generous enough for an office behind one IP.

/** Rejects a client that's over its limit for `scope`, with a 429 the app
 * treats as offline. Without the binding (local tests), nothing is limited. */
export function rateLimit(binding: keyof Pick<Env, "LICENSE_LIMITER" | "CHECKOUT_LIMITER">, scope: string): MiddlewareHandler<AppEnv> {
  return async (c, next) => {
    const limiter = c.env[binding];
    if (limiter) {
      // Cloudflare sets this from the connection; a client can't override it.
      const ip = c.req.header("cf-connecting-ip") ?? "unknown";
      const { success } = await limiter.limit({ key: `${scope}:${ip}` });
      if (!success) {
        c.header("retry-after", "60");
        return fail(c, 429, "rate_limited", "Too many requests. Try again in a minute.");
      }
    }
    await next();
  };
}

/** Caps request bodies: license and checkout calls are a few hundred bytes. */
export const smallBody = bodyLimit({
  maxSize: 16 * 1024,
  onError: (c) => fail(c, 413, "too_large", "That request is too large."),
});

/** Dodo's webhook payloads are larger, but still small. */
export const webhookBody = bodyLimit({
  maxSize: 512 * 1024,
  onError: (c) => fail(c, 413, "too_large", "That request is too large."),
});
