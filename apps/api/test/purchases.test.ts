import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { Webhook } from "standardwebhooks";
import app from "../src/index";
import type { Env } from "../src/env";
import { hashKey } from "../src/keys";
import { mockDodo, post, testEnv } from "./helpers";

// A paid order gets a license key dotsafe issues itself, emailed exactly once;
// a full refund revokes it. Dodo is only the payment: no call reaches it here
// (mockDodo with no responses fails the test on any fetch).

let t: ReturnType<typeof testEnv>;
beforeEach(() => {
  t = testEnv();
  mockDodo();
  vi.spyOn(console, "log").mockImplementation(() => {});
  vi.spyOn(console, "error").mockImplementation(() => {});
});
afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

let seq = 0;
function deliver(type: string, data: Record<string, unknown>, id = `msg_${++seq}`) {
  const body = JSON.stringify({ business_id: "bus_1", type, timestamp: "2026-11-03T10:00:00Z", data });
  const now = new Date();
  return app.request(
    post("/v1/webhooks/dodo", body, {
      "webhook-id": id,
      "webhook-timestamp": String(Math.floor(now.getTime() / 1000)),
      "webhook-signature": new Webhook(t.env.DODO_WEBHOOK_KEY).sign(id, now, body),
    }),
    undefined,
    t.env,
  );
}

const payment = {
  payment_id: "pay_1",
  customer: { customer_id: "cus_1", email: "ada@example.com", name: "Ada Lovelace" },
  total_amount: 4900,
  currency: "USD",
  created_at: "2026-11-03T09:59:00Z",
  product_cart: [{ product_id: "pdt_dotsafe", quantity: 1 }],
};
const row = (sql: string) => t.sqlite.prepare(sql).get() as any;
const sentKey = (i = 0) => (t.sent[i].text as string).match(/DOTSAFE(-[0-9A-Z]{5}){4}/)![0];

describe("purchase webhooks", () => {
  it("records the payment, issues a key and emails it", async () => {
    expect((await deliver("payment.succeeded", payment)).status).toBe(200);
    expect(row("SELECT * FROM purchases")).toMatchObject({ payment_id: "pay_1", email: "ada@example.com", amount: 4900 });
    expect(row("SELECT * FROM licenses")).toMatchObject({
      email: "ada@example.com",
      name: "Ada Lovelace",
      source: "purchase",
      payment_id: "pay_1",
      activations_limit: 2,
    });
    expect(t.sent).toHaveLength(1);
    expect(t.sent[0]).toMatchObject({ to: "ada@example.com", subject: "Your dotsafe license key" });
    expect(t.sent[0].text).toContain("Hi Ada,");
    // The emailed key is the one stored (as a hash) and it activates.
    expect(row("SELECT key_hash FROM licenses").key_hash).toBe(await hashKey(sentKey()));
    const res = await app.request(post("/v1/licenses/activate", { license_key: sentKey() }), undefined, t.env);
    expect(res.status).toBe(201);
  });

  it("issues one license per payment, however often the webhook comes", async () => {
    await deliver("payment.succeeded", payment, "msg_same");
    await deliver("payment.succeeded", payment, "msg_same");
    await deliver("payment.succeeded", payment);
    expect(row("SELECT count(*) AS n FROM licenses").n).toBe(1);
    expect(t.sent).toHaveLength(1);
  });

  it("answers 500 when the email fails, then sends a working key on the retry", async () => {
    const send = t.env.EMAIL.send;
    t.env.EMAIL.send = async () => {
      throw new Error("email service down");
    };
    expect((await deliver("payment.succeeded", payment, "msg_retry")).status).toBe(500);
    expect(row("SELECT emailed_at FROM licenses").emailed_at).toBeNull();

    t.env.EMAIL.send = send;
    expect((await deliver("payment.succeeded", payment, "msg_retry")).status).toBe(200);
    expect(t.sent).toHaveLength(1);
    expect(row("SELECT count(*) AS n FROM licenses").n).toBe(1);
    expect(row("SELECT key_hash FROM licenses").key_hash).toBe(await hashKey(sentKey()));
  });

  it("ignores payments for other products", async () => {
    await deliver("payment.succeeded", { ...payment, product_cart: [{ product_id: "pdt_other", quantity: 1 }] });
    expect(row("SELECT count(*) AS n FROM licenses").n).toBe(0);
    expect(t.sent).toHaveLength(0);
  });

  it("revokes the key on a full refund, not a partial one, and records disputes", async () => {
    await deliver("payment.succeeded", payment);
    await deliver("refund.succeeded", { refund_id: "ref_1", payment_id: "pay_1", amount: 1000, created_at: "2026-11-04T00:00:00Z" });
    expect(row("SELECT revoked_at FROM licenses").revoked_at).toBeNull();

    await deliver("refund.succeeded", { refund_id: "ref_2", payment_id: "pay_1", amount: 3900, created_at: "2026-11-05T00:00:00Z" });
    expect(row("SELECT revoked_at, revoked_reason FROM licenses")).toEqual({ revoked_at: "2026-11-05T00:00:00Z", revoked_reason: "refund" });
    const res = await app.request(post("/v1/licenses/activate", { license_key: sentKey() }), undefined, t.env);
    expect(res.status).toBe(403);

    await deliver("dispute.opened", { dispute_id: "dsp_1", payment_id: "pay_1", created_at: "2026-11-06T00:00:00Z" });
    expect(row("SELECT disputed_at FROM purchases").disputed_at).toBe("2026-11-06T00:00:00Z");
  });

  it("rejects a forged webhook before reading it", async () => {
    const body = JSON.stringify({ type: "payment.succeeded", data: payment });
    const res = await app.request(
      post("/v1/webhooks/dodo", body, { "webhook-id": "msg_x", "webhook-timestamp": String(Math.floor(Date.now() / 1000)), "webhook-signature": "v1,AAAA" }),
      undefined,
      t.env,
    );
    expect(res.status).toBe(401);
    expect(row("SELECT count(*) AS n FROM licenses").n).toBe(0);
  });
});

describe("POST /v1/admin/licenses", () => {
  const admin = (body: unknown, token?: string, env: Env = { ...t.env, ADMIN_TOKEN: "s3cret-admin-token" }) =>
    app.request(post("/v1/admin/licenses", body, token ? { authorization: `Bearer ${token}` } : {}), undefined, env);

  it("issues and emails a free license with the admin token", async () => {
    const res = await admin({ email: "saurabh@example.com", name: "Saurabh" }, "s3cret-admin-token");
    expect(res.status).toBe(201);
    const json = (await res.json()) as any;
    expect(json.key).toBe(sentKey());
    expect(json.key_hint).toBe(sentKey().slice(-4));
    expect(row("SELECT source, email, payment_id FROM licenses")).toEqual({ source: "manual", email: "saurabh@example.com", payment_id: null });
    expect(t.sent[0].to).toBe("saurabh@example.com");
  });

  it("refuses without the right token, and is off when no token is set", async () => {
    expect((await admin({ email: "x@example.com" })).status).toBe(401);
    expect((await admin({ email: "x@example.com" }, "wrong")).status).toBe(401);
    expect((await admin({ email: "x@example.com" }, "anything", t.env)).status).toBe(404);
    expect(row("SELECT count(*) AS n FROM licenses").n).toBe(0);
  });

  it("needs a valid email", async () => {
    expect((await admin({ email: "not-an-email" }, "s3cret-admin-token")).status).toBe(400);
  });
});
