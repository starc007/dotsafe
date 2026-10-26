import type { Env } from "./env";

// A small typed client for the Dodo Payments endpoints this API uses.
// Plain fetch keeps the Worker small; see docs.dodopayments.com/api-reference.

const BASE_URL = { test_mode: "https://test.dodopayments.com", live_mode: "https://live.dodopayments.com" } as const;

/** A non-2xx answer from Dodo, with its status so callers can map it. */
export class DodoError extends Error {
  constructor(
    public status: number,
    public body: string,
  ) {
    super(`Dodo Payments returned ${status}`);
  }
}

export type LicenseInstance = { id: string; license_key_id: string; business_id: string; created_at: string };

export type LicenseKey = {
  id: string;
  product_id: string;
  status: "active" | "expired" | "disabled" | string;
  activations_limit: number | null;
  instances_count: number;
  expires_at: string | null;
};

export type CheckoutSession = { session_id: string; checkout_url: string | null };

export function dodo(env: Env) {
  const base = BASE_URL[env.DODO_MODE] ?? BASE_URL.test_mode;

  async function request<T>(method: string, path: string, body?: unknown, auth = false): Promise<T> {
    const headers: Record<string, string> = { accept: "application/json" };
    if (body !== undefined) headers["content-type"] = "application/json";
    if (auth) headers.authorization = `Bearer ${env.DODO_API_KEY}`;
    const res = await fetch(`${base}${path}`, {
      method,
      headers,
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    const text = await res.text();
    if (!res.ok) throw new DodoError(res.status, text);
    return (text ? JSON.parse(text) : {}) as T;
  }

  return {
    /** Public endpoint: records this device against the key. */
    activate: (licenseKey: string, name: string) =>
      request<LicenseInstance>("POST", "/licenses/activate", { license_key: licenseKey, name }),

    /** Public endpoint: true while the key is active, unexpired and the instance exists. */
    validate: (licenseKey: string, instanceId: string) =>
      request<{ valid: boolean }>("POST", "/licenses/validate", {
        license_key: licenseKey,
        license_key_instance_id: instanceId,
      }),

    /** Public endpoint: frees one activation. */
    deactivate: (licenseKey: string, instanceId: string) =>
      request<unknown>("POST", "/licenses/deactivate", {
        license_key: licenseKey,
        license_key_instance_id: instanceId,
      }),

    /** Authenticated: full key details, including which product it's for. */
    getLicenseKey: (id: string) => request<LicenseKey>("GET", `/license_keys/${encodeURIComponent(id)}`, undefined, true),

    /** Authenticated: a hosted checkout for one copy of dotsafe. */
    createCheckout: (args: { productId: string; returnUrl: string; email?: string }) =>
      request<CheckoutSession>(
        "POST",
        "/checkouts",
        {
          product_cart: [{ product_id: args.productId, quantity: 1 }],
          return_url: args.returnUrl,
          ...(args.email ? { customer: { email: args.email } } : {}),
        },
        true,
      ),
  };
}
