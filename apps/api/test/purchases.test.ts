import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { Webhook } from "standardwebhooks";
import app from "../src/index";
import { mockDodo, post, testEnv } from "./helpers";

// Purchases, license keys and devices are recorded from webhooks and the
// license routes, and each new key is emailed exactly once.

let t: ReturnType<typeof testEnv>;
beforeEach(() => {
  t = testEnv();
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
const grant = {
  id: "grant_1",
  customer_id: "cus_1",
  payment_id: "pay_1",
  status: "Delivered",
  integration_type: "license_key",
  license_key: { id: "lic_1", key: "DS-AAAA-BBBB", status: "active", expires_at: null, activations_used: 0, activations_limit: 2 },
  created_at: "2026-11-03T10:00:00Z",
};
const keyDetails = { id: "lic_1", product_id: "pdt_dotsafe", status: "active", activations_limit: 2, instances_count: 0, expires_at: null };
const row = (sql: string) => t.sqlite.prepare(sql).get() as any;

describe("purchase webhooks", () => {
  it("records the payment, then the key, and emails the key once", async () => {
    expect((await deliver("payment.succeeded", payment)).status).toBe(200);
    expect(row("SELECT * FROM purchases")).toMatchObject({
      payment_id: "pay_1",
      email: "ada@example.com",
      name: "Ada Lovelace",
      amount: 4900,
      currency: "USD",
      paid_at: "2026-11-03T09:59:00Z",
    });

    // Only the product check reaches Dodo: the buyer is already known.
    const calls = mockDodo({ status: 200, body: keyDetails });
    expect((await deliver("entitlement_grant.created", grant)).status).toBe(200);
    expect(calls).toHaveLength(1);
    expect(row("SELECT * FROM licenses")).toMatchObject({
      id: "lic_1",
      key: "DS-AAAA-BBBB",
      payment_id: "pay_1",
      email: "ada@example.com",
      activations_limit: 2,
    });
    expect(row("SELECT emailed_at FROM licenses").emailed_at).toBeTruthy();
    expect(t.sent).toHaveLength(1);
    expect(t.sent[0]).toMatchObject({ to: "ada@example.com", subject: "Your dotsafe license key" });
    expect(t.sent[0].text).toContain("DS-AAAA-BBBB");
    expect(t.sent[0].text).toContain("Hi Ada,");
  });

  it("asks Dodo for the buyer when the key arrives first", async () => {
    mockDodo({ status: 200, body: keyDetails }, { status: 200, body: { customer_id: "cus_1", email: "ada@example.com", name: null } });
    await deliver("entitlement_grant.created", grant);
    expect(t.sent[0].to).toBe("ada@example.com");

    await deliver("payment.succeeded", payment);
    expect(row("SELECT count(*) AS n FROM purchases").n).toBe(1);
    expect(t.sent).toHaveLength(1);
  });

  it("handles a redelivered webhook once", async () => {
    mockDodo({ status: 200, body: keyDetails }, { status: 200, body: { customer_id: "cus_1", email: "ada@example.com", name: null } });
    await deliver("entitlement_grant.created", grant, "msg_same");
    expect((await deliver("entitlement_grant.created", grant, "msg_same")).status).toBe(200);
    expect(t.sent).toHaveLength(1);
  });

  it("never emails a key twice, even from different deliveries", async () => {
    await deliver("payment.succeeded", payment);
    mockDodo({ status: 200, body: keyDetails }, { status: 200, body: keyDetails });
    await deliver("entitlement_grant.created", grant);
    await deliver("entitlement_grant.delivered", grant);
    expect(t.sent).toHaveLength(1);
  });

  it("answers 500 when the email fails, so Dodo retries, and sends on the retry", async () => {
    await deliver("payment.succeeded", payment);
    const send = t.env.EMAIL.send;
    t.env.EMAIL.send = async () => {
      throw new Error("email service down");
    };
    mockDodo({ status: 200, body: keyDetails }, { status: 200, body: keyDetails });
    expect((await deliver("entitlement_grant.created", grant, "msg_retry")).status).toBe(500);
    expect(row("SELECT emailed_at FROM licenses").emailed_at).toBeNull();

    t.env.EMAIL.send = send;
    expect((await deliver("entitlement_grant.created", grant, "msg_retry")).status).toBe(200);
    expect(t.sent).toHaveLength(1);
  });

  it("ignores keys for other products", async () => {
    mockDodo({ status: 200, body: { ...keyDetails, product_id: "pdt_other" } });
    await deliver("entitlement_grant.created", grant);
    expect(row("SELECT count(*) AS n FROM licenses").n).toBe(0);
    expect(t.sent).toHaveLength(0);
  });

  it("records refunds, disputes and revoked keys", async () => {
    await deliver("payment.succeeded", payment);
    mockDodo({ status: 200, body: keyDetails });
    await deliver("entitlement_grant.created", grant);

    await deliver("refund.succeeded", { refund_id: "ref_1", payment_id: "pay_1", amount: 4900, created_at: "2026-11-04T00:00:00Z" });
    await deliver("dispute.opened", { dispute_id: "dsp_1", payment_id: "pay_1", created_at: "2026-11-05T00:00:00Z" });
    await deliver("entitlement_grant.revoked", { ...grant, status: "Revoked", revoked_at: "2026-11-04T00:00:01Z", revocation_reason: "refund" });

    expect(row("SELECT * FROM purchases")).toMatchObject({
      refunded_amount: 4900,
      refunded_at: "2026-11-04T00:00:00Z",
      disputed_at: "2026-11-05T00:00:00Z",
    });
    expect(row("SELECT * FROM licenses")).toMatchObject({ revoked_at: "2026-11-04T00:00:01Z", revoked_reason: "refund" });
  });
});

describe("device records", () => {
  const instance = { id: "lki_1", license_key_id: "lic_1", business_id: "bus_1", created_at: "2026-11-03T11:00:00Z" };
  const req = (path: string, body: unknown, env = t.env) => app.request(post(path, body), undefined, env);

  it("records activation, each check and deactivation", async () => {
    mockDodo({ status: 201, body: instance }, { status: 200, body: { ...keyDetails, instances_count: 1 } });
    await req("/v1/licenses/activate", { license_key: "DS-AAAA-BBBB", device_name: "Ada's MacBook" });
    expect(row("SELECT * FROM activations")).toMatchObject({
      instance_id: "lki_1",
      license_id: "lic_1",
      device_name: "Ada's MacBook",
      activated_at: "2026-11-03T11:00:00Z",
      deactivated_at: null,
    });

    mockDodo({ status: 200, body: { valid: true } });
    const before = row("SELECT last_seen_at FROM activations").last_seen_at;
    await req("/v1/licenses/validate", { license_key: "DS-AAAA-BBBB", instance_id: "lki_1" });
    expect(row("SELECT last_seen_at FROM activations").last_seen_at).not.toBe(before);

    mockDodo({ status: 200 });
    await req("/v1/licenses/deactivate", { license_key: "DS-AAAA-BBBB", instance_id: "lki_1" });
    expect(row("SELECT deactivated_at FROM activations").deactivated_at).toBeTruthy();
  });

  it("still activates when the database is unavailable", async () => {
    mockDodo({ status: 201, body: instance }, { status: 200, body: keyDetails });
    const res = await req("/v1/licenses/activate", { license_key: "DS-AAAA-BBBB" }, { ...t.env, DB: undefined as never });
    expect(res.status).toBe(201);
  });
});
