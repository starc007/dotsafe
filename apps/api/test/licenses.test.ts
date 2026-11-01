import { afterEach, describe, expect, it, vi } from "vitest";
import app from "../src/index";
import { env, mockDodo, post } from "./helpers";

const instance = { id: "lki_1", license_key_id: "lic_1", business_id: "bus_1", created_at: "2026-10-26T10:00:00Z" };
const key = { id: "lic_1", product_id: "pdt_dotsafe", status: "active", activations_limit: 3, instances_count: 1, expires_at: null };

afterEach(() => vi.unstubAllGlobals());

async function activate(body: unknown, e = env) {
  const res = await app.request(post("/v1/licenses/activate", body), undefined, e);
  return { status: res.status, json: (await res.json()) as any };
}

describe("POST /v1/licenses/activate", () => {
  it("activates a dotsafe key and reports activations", async () => {
    const calls = mockDodo({ status: 201, body: instance }, { status: 200, body: key });
    const { status, json } = await activate({ license_key: "  DOTSAFE-1234  ", device_name: "Saurabh's MacBook" });

    expect(status).toBe(201);
    expect(json).toEqual({
      instance_id: "lki_1",
      activated_at: "2026-10-26T10:00:00Z",
      activations: { used: 1, limit: 3 },
      expires_at: null,
    });
    expect(calls[0].url).toBe("https://test.dodopayments.com/licenses/activate");
    expect(calls[0].body).toEqual({ license_key: "DOTSAFE-1234", name: "Saurabh's MacBook" });
    // The public endpoint gets no credentials; only the key lookup is authenticated.
    expect(calls[0].headers.authorization).toBeUndefined();
    expect(calls[1].url).toBe("https://test.dodopayments.com/license_keys/lic_1");
    expect(calls[1].headers.authorization).toBe("Bearer dodo_test_secret");
  });

  it("uses the live host in live mode", async () => {
    const calls = mockDodo({ status: 201, body: instance }, { status: 200, body: key });
    await activate({ license_key: "K-1234567" }, { ...env, DODO_MODE: "live_mode" });
    expect(calls[0].url).toBe("https://live.dodopayments.com/licenses/activate");
  });

  it("rejects a key for another product and frees the activation", async () => {
    const calls = mockDodo(
      { status: 201, body: instance },
      { status: 200, body: { ...key, product_id: "pdt_someone_else" } },
      { status: 200 },
    );
    const { status, json } = await activate({ license_key: "OTHER-KEY-1" });
    expect(status).toBe(403);
    expect(json.error.code).toBe("wrong_product");
    expect(calls[2].url).toBe("https://test.dodopayments.com/licenses/deactivate");
    expect(calls[2].body).toEqual({ license_key: "OTHER-KEY-1", license_key_instance_id: "lki_1" });
  });

  it.each([
    [404, 404, "not_found"],
    [403, 403, "inactive"],
    [422, 409, "limit_reached"],
    [500, 502, "upstream_error"],
  ])("maps Dodo %i to %i %s", async (dodoStatus, status, code) => {
    mockDodo({ status: dodoStatus, body: { message: "nope" } });
    const res = await activate({ license_key: "DOTSAFE-1234" });
    expect(res.status).toBe(status);
    expect(res.json.error.code).toBe(code);
  });

  it("reports network failures as upstream errors", async () => {
    mockDodo(new TypeError("network down"));
    const res = await activate({ license_key: "DOTSAFE-1234" });
    expect(res.status).toBe(502);
    expect(res.json.error.code).toBe("upstream_error");
  });

  it("frees the activation if the key lookup fails", async () => {
    const calls = mockDodo({ status: 201, body: instance }, { status: 500 }, { status: 200 });
    const res = await activate({ license_key: "DOTSAFE-1234" });
    expect(res.status).toBe(502);
    expect(calls[2].url).toContain("/licenses/deactivate");
  });

  it("requires a key", async () => {
    mockDodo();
    for (const body of [{}, { license_key: "   " }, "not json", { license_key: "x".repeat(200) }]) {
      const res = await activate(body);
      expect(res.status).toBe(400);
      expect(res.json.error.code).toBe("invalid_request");
    }
  });

  it("refuses to activate without a configured product", async () => {
    mockDodo();
    const res = await activate({ license_key: "DOTSAFE-1234" }, { ...env, DODO_PRODUCT_ID: "" });
    expect(res.status).toBe(503);
    expect(res.json.error.code).toBe("not_configured");
  });
});

describe("POST /v1/licenses/validate", () => {
  it("passes Dodo's answer through", async () => {
    const calls = mockDodo({ status: 200, body: { valid: false } });
    const res = await app.request(post("/v1/licenses/validate", { license_key: "K-1234567", instance_id: "lki_1" }), undefined, env);
    expect(await res.json()).toEqual({ valid: false });
    expect(calls[0].body).toEqual({ license_key: "K-1234567", license_key_instance_id: "lki_1" });
  });

  it("needs the instance id", async () => {
    mockDodo();
    const res = await app.request(post("/v1/licenses/validate", { license_key: "K-1234567" }), undefined, env);
    expect(res.status).toBe(400);
  });

  it("reports an unreachable Dodo as 502 so the app can stay offline-friendly", async () => {
    mockDodo(new TypeError("offline"));
    const res = await app.request(post("/v1/licenses/validate", { license_key: "K-1234567", instance_id: "lki_1" }), undefined, env);
    expect(res.status).toBe(502);
  });
});

describe("POST /v1/licenses/deactivate", () => {
  it("frees the activation", async () => {
    mockDodo({ status: 200 });
    const res = await app.request(post("/v1/licenses/deactivate", { license_key: "K-1234567", instance_id: "lki_1" }), undefined, env);
    expect(await res.json()).toEqual({ ok: true });
  });

  it("treats an already-removed activation as done", async () => {
    mockDodo({ status: 403 });
    const res = await app.request(post("/v1/licenses/deactivate", { license_key: "K-1234567", instance_id: "lki_gone" }), undefined, env);
    expect(res.status).toBe(200);
  });
});
