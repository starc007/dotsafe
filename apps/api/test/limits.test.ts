import { afterEach, describe, expect, it, vi } from "vitest";
import app from "../src/index";
import type { Env } from "../src/env";
import { env, mockDodo, post } from "./helpers";

afterEach(() => vi.unstubAllGlobals());

/** A rate limiter that allows `limit` calls per key, and records the keys. */
function fakeLimiter(limit: number) {
  const counts = new Map<string, number>();
  return {
    keys: counts,
    binding: {
      limit: async ({ key }: { key: string }) => {
        counts.set(key, (counts.get(key) ?? 0) + 1);
        return { success: counts.get(key)! <= limit };
      },
    } as unknown as RateLimit,
  };
}

const from = (ip: string) => ({ "cf-connecting-ip": ip });

describe("abuse limits", () => {
  it("answers 429 once a client is over its limit, per IP, without calling Dodo", async () => {
    const limiter = fakeLimiter(1);
    const limited: Env = { ...env, LICENSE_LIMITER: limiter.binding };
    const calls = mockDodo({ status: 200, body: { valid: true } }, { status: 200, body: { valid: true } });

    const body = { license_key: "K-1", instance_id: "lki_1" };
    expect((await app.request(post("/v1/licenses/validate", body, from("1.1.1.1")), undefined, limited)).status).toBe(200);
    const blocked = await app.request(post("/v1/licenses/validate", body, from("1.1.1.1")), undefined, limited);
    expect(blocked.status).toBe(429);
    expect(blocked.headers.get("retry-after")).toBe("60");
    expect(await blocked.json()).toEqual({ error: { code: "rate_limited", message: "Too many requests. Try again in a minute." } });

    // Another client still gets through, and the blocked call never reached Dodo.
    expect((await app.request(post("/v1/licenses/validate", body, from("2.2.2.2")), undefined, limited)).status).toBe(200);
    expect(calls).toHaveLength(2);
    expect([...limiter.keys.keys()]).toEqual(["licenses:1.1.1.1", "licenses:2.2.2.2"]);
  });

  it("limits checkouts on their own counter", async () => {
    const limiter = fakeLimiter(0);
    const limited: Env = { ...env, CHECKOUT_LIMITER: limiter.binding };
    const calls = mockDodo();
    const res = await app.request(post("/v1/checkout", {}, from("3.3.3.3")), undefined, limited);
    expect(res.status).toBe(429);
    expect(calls).toHaveLength(0);
    expect([...limiter.keys.keys()]).toEqual(["checkout:3.3.3.3"]);
  });

  it("refuses oversized bodies before doing anything", async () => {
    const calls = mockDodo();
    const huge = { license_key: "K".repeat(20_000) };
    const res = await app.request(post("/v1/licenses/activate", huge), undefined, env);
    expect(res.status).toBe(413);
    expect(((await res.json()) as { error: { code: string } }).error.code).toBe("too_large");
    expect(calls).toHaveLength(0);
  });
});
