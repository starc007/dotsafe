import type { Env } from "./env";

// A small typed client for the one Dodo Payments endpoint this API calls:
// starting a checkout. Dodo only takes payments; license keys are dotsafe's own
// (src/keys.ts, src/issue.ts). See docs.dodopayments.com/api-reference.

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

export type CheckoutSession = { session_id: string; checkout_url: string | null };

export function dodo(env: Env) {
  const base = BASE_URL[env.DODO_MODE] ?? BASE_URL.test_mode;

  return {
    /** A hosted checkout for one copy of dotsafe. */
    async createCheckout(args: { productId: string; returnUrl: string; email?: string }): Promise<CheckoutSession> {
      const res = await fetch(`${base}/checkouts`, {
        method: "POST",
        headers: { accept: "application/json", "content-type": "application/json", authorization: `Bearer ${env.DODO_API_KEY}` },
        body: JSON.stringify({
          product_cart: [{ product_id: args.productId, quantity: 1 }],
          return_url: args.returnUrl,
          // Only the country (and ZIP where tax needs it): a download needs no address.
          minimal_address: true,
          ...(args.email ? { customer: { email: args.email } } : {}),
        }),
      });
      const text = await res.text();
      if (!res.ok) throw new DodoError(res.status, text);
      return (text ? JSON.parse(text) : {}) as CheckoutSession;
    },
  };
}
