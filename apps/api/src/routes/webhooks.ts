import { Hono } from "hono";
import { Webhook } from "standardwebhooks";
import type { AppEnv } from "../env";

/** Dodo Payments webhooks (Standard Webhooks signatures). Verified before anything is read. */
export const webhooks = new Hono<AppEnv>();

type DodoEvent = { type: string; business_id?: string; data?: Record<string, unknown> };

webhooks.post("/dodo", async (c) => {
  // Verify against the raw body exactly as sent; parsing first can change it.
  const raw = await c.req.text();
  let event: DodoEvent;
  try {
    new Webhook(c.env.DODO_WEBHOOK_KEY).verify(raw, {
      "webhook-id": c.req.header("webhook-id") ?? "",
      "webhook-timestamp": c.req.header("webhook-timestamp") ?? "",
      "webhook-signature": c.req.header("webhook-signature") ?? "",
    });
    event = JSON.parse(raw) as DodoEvent;
  } catch {
    return c.json({ error: { code: "invalid_signature", message: "Invalid webhook signature." } }, 401);
  }

  // Nothing is stored yet; these logs are the purchase record until the
  // website has a database. Never log full license keys.
  const id = c.req.header("webhook-id");
  switch (event.type) {
    case "payment.succeeded":
    case "refund.succeeded":
    case "dispute.opened":
      console.log(JSON.stringify({ webhook: event.type, id, payment_id: event.data?.payment_id }));
      break;
    case "license_key.created":
      console.log(JSON.stringify({ webhook: event.type, id, license_key_id: event.data?.id, product_id: event.data?.product_id }));
      break;
    default:
      console.log(JSON.stringify({ webhook: event.type, id, ignored: true }));
  }
  return c.json({ received: true });
});
