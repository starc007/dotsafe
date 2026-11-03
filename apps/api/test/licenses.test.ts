import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import app from "../src/index";
import { issueLicense } from "../src/issue";
import { hashKey, newKey } from "../src/keys";
import { mockDodo, post, testEnv } from "./helpers";

// Activation runs entirely on dotsafe's own keys in D1: no payment provider
// is called (mockDodo with no responses fails the test on any fetch).

let t: ReturnType<typeof testEnv>;
let key: string;
beforeEach(async () => {
  t = testEnv();
  mockDodo();
  key = (await issueLicense(t.env, { email: "ada@example.com", name: "Ada", source: "manual", paymentId: null })).key!;
});
afterEach(() => vi.unstubAllGlobals());

async function call(path: string, body: unknown) {
  const res = await app.request(post(path, body), undefined, t.env);
  return { status: res.status, json: (await res.json()) as any };
}
const activate = (body: unknown) => call("/v1/licenses/activate", body);
const row = (sql: string) => t.sqlite.prepare(sql).get() as any;

describe("keys", () => {
  it("look like DOTSAFE-XXXXX-XXXXX-XXXXX-XXXXX and are stored only as a hash", async () => {
    expect(key).toMatch(/^DOTSAFE(-[0-9A-HJKMNP-TV-Z]{5}){4}$/);
    const stored = row("SELECT key_hash, key_hint FROM licenses");
    expect(stored.key_hash).toBe(await hashKey(key));
    expect(stored.key_hint).toBe(key.slice(-4));
    expect(JSON.stringify(t.sqlite.prepare("SELECT * FROM licenses").all())).not.toContain(key);
  });

  it("are different every time", () => {
    expect(new Set(Array.from({ length: 50 }, newKey)).size).toBe(50);
  });
});

describe("POST /v1/licenses/activate", () => {
  it("activates a key, case- and space-insensitively, and reports the slots", async () => {
    const { status, json } = await activate({ license_key: `  ${key.toLowerCase()}  `, device_name: "Ada's MacBook" });
    expect(status).toBe(201);
    expect(json.instance_id).toMatch(/^act_/);
    expect(json.activations).toEqual({ used: 1, limit: 2 });
    expect(row("SELECT device_name FROM activations").device_name).toBe("Ada's MacBook");
  });

  it("stops at the device limit", async () => {
    expect((await activate({ license_key: key })).status).toBe(201);
    expect((await activate({ license_key: key })).status).toBe(201);
    const third = await activate({ license_key: key });
    expect(third.status).toBe(409);
    expect(third.json.error.code).toBe("limit_reached");
  });

  it("frees a slot when a device is deactivated", async () => {
    const first = await activate({ license_key: key });
    await activate({ license_key: key });
    await call("/v1/licenses/deactivate", { license_key: key, instance_id: first.json.instance_id });
    expect((await activate({ license_key: key })).status).toBe(201);
  });

  it("rejects unknown and revoked keys", async () => {
    const unknown = await activate({ license_key: newKey() });
    expect(unknown.status).toBe(404);
    expect(unknown.json.error.code).toBe("not_found");

    t.sqlite.prepare("UPDATE licenses SET revoked_at = '2026-11-04T00:00:00Z'").run();
    const revoked = await activate({ license_key: key });
    expect(revoked.status).toBe(403);
    expect(revoked.json.error.code).toBe("inactive");
  });

  it("requires a key", async () => {
    for (const body of [{}, { license_key: "   " }, "not json", { license_key: "x".repeat(200) }]) {
      const res = await activate(body);
      expect(res.status).toBe(400);
      expect(res.json.error.code).toBe("invalid_request");
    }
  });
});

describe("POST /v1/licenses/validate", () => {
  it("is valid for an active device of a good key, and records the check", async () => {
    const { json } = await activate({ license_key: key });
    t.sqlite.prepare("UPDATE activations SET last_seen_at = '2000-01-01T00:00:00Z'").run();
    expect((await call("/v1/licenses/validate", { license_key: key, instance_id: json.instance_id })).json).toEqual({ valid: true });
    expect(row("SELECT last_seen_at FROM activations").last_seen_at).not.toBe("2000-01-01T00:00:00Z");
  });

  it("is invalid after revocation or deactivation, or for someone else's device", async () => {
    const { json } = await activate({ license_key: key });
    const other = (await issueLicense(t.env, { email: "bob@example.com", name: null, source: "manual", paymentId: null })).key!;
    expect((await call("/v1/licenses/validate", { license_key: other, instance_id: json.instance_id })).json).toEqual({ valid: false });

    t.sqlite.prepare("UPDATE licenses SET revoked_at = '2026-11-04T00:00:00Z' WHERE email = 'ada@example.com'").run();
    expect((await call("/v1/licenses/validate", { license_key: key, instance_id: json.instance_id })).json).toEqual({ valid: false });
  });

  it("needs the instance id", async () => {
    expect((await call("/v1/licenses/validate", { license_key: key })).status).toBe(400);
  });
});

describe("POST /v1/licenses/deactivate", () => {
  it("frees the device and answers the same for anything else", async () => {
    const { json } = await activate({ license_key: key });
    expect((await call("/v1/licenses/deactivate", { license_key: key, instance_id: json.instance_id })).json).toEqual({ ok: true });
    expect(row("SELECT deactivated_at FROM activations").deactivated_at).toBeTruthy();
    expect((await call("/v1/licenses/deactivate", { license_key: newKey(), instance_id: "act_nope" })).json).toEqual({ ok: true });
  });

  it("never lets another key free a device", async () => {
    const { json } = await activate({ license_key: key });
    await call("/v1/licenses/deactivate", { license_key: newKey(), instance_id: json.instance_id });
    const other = (await issueLicense(t.env, { email: "bob@example.com", name: null, source: "manual", paymentId: null })).key!;
    await call("/v1/licenses/deactivate", { license_key: other, instance_id: json.instance_id });
    expect(row("SELECT deactivated_at FROM activations").deactivated_at).toBeNull();
  });
});
