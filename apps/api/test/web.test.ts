import { afterEach, describe, expect, it, vi } from "vitest";
import { Webhook } from "standardwebhooks";
import app from "../src/index";
import { env, mockDodo, post } from "./helpers";

afterEach(() => vi.unstubAllGlobals());

describe("POST /v1/checkout", () => {
  it("creates a checkout for the dotsafe product", async () => {
    const calls = mockDodo({ status: 200, body: { session_id: "cks_1", checkout_url: "https://test.checkout.dodopayments.com/session/cks_1" } });
    const res = await app.request(post("/v1/checkout", { email: "buyer@example.com" }), undefined, env);
    expect(await res.json()).toEqual({ checkout_url: "https://test.checkout.dodopayments.com/session/cks_1" });
    expect(calls[0].url).toBe("https://test.dodopayments.com/checkouts");
    expect(calls[0].headers.authorization).toBe("Bearer dodo_test_secret");
    expect(calls[0].body).toEqual({
      product_cart: [{ product_id: "pdt_dotsafe", quantity: 1 }],
      return_url: "https://dotsafe.dev/thanks",
      customer: { email: "buyer@example.com" },
    });
  });

  it("allows the website origin and no other", async () => {
    mockDodo({ status: 200, body: { session_id: "cks_1", checkout_url: "https://x" } }, { status: 200, body: { session_id: "cks_2", checkout_url: "https://y" } });
    const ok = await app.request(post("/v1/checkout", {}, { origin: "https://dotsafe.dev" }), undefined, env);
    expect(ok.headers.get("access-control-allow-origin")).toBe("https://dotsafe.dev");
    const other = await app.request(post("/v1/checkout", {}, { origin: "https://evil.example" }), undefined, env);
    expect(other.headers.get("access-control-allow-origin")).toBeNull();
  });
});

describe("POST /v1/webhooks/dodo", () => {
  const event = { business_id: "bus_1", type: "payment.succeeded", data: { payment_id: "pay_1" } };

  function signed(body: string, at = new Date()) {
    const id = "msg_1";
    const signature = new Webhook(env.DODO_WEBHOOK_KEY).sign(id, at, body);
    return post("/v1/webhooks/dodo", body, {
      "webhook-id": id,
      "webhook-timestamp": String(Math.floor(at.getTime() / 1000)),
      "webhook-signature": signature,
    });
  }

  it("accepts a correctly signed event", async () => {
    vi.spyOn(console, "log").mockImplementation(() => {});
    const res = await app.request(signed(JSON.stringify(event)), undefined, env);
    expect(res.status).toBe(200);
    expect(await res.json()).toEqual({ received: true });
  });

  it("rejects a tampered body", async () => {
    const req = signed(JSON.stringify(event));
    const tampered = new Request(req, { body: JSON.stringify({ ...event, type: "refund.succeeded" }) });
    const res = await app.request(tampered, undefined, env);
    expect(res.status).toBe(401);
  });

  it("rejects an old replayed event", async () => {
    const res = await app.request(signed(JSON.stringify(event), new Date(Date.now() - 60 * 60 * 1000)), undefined, env);
    expect(res.status).toBe(401);
  });

  it("rejects unsigned requests", async () => {
    const res = await app.request(post("/v1/webhooks/dodo", event), undefined, env);
    expect(res.status).toBe(401);
  });
});

describe("misc", () => {
  it("has a health check and JSON 404s", async () => {
    expect(await (await app.request("/health", undefined, env)).json()).toEqual({ ok: true });
    const missing = await app.request("/nope", undefined, env);
    expect(missing.status).toBe(404);
    expect(((await missing.json()) as any).error.code).toBe("not_found");
  });
});
