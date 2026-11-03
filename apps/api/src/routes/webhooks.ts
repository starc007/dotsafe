import { Hono } from "hono";
import { Webhook } from "standardwebhooks";
import type { AppEnv, Env } from "../env";
import { records } from "../db";
import { type DotsafeEvent, parseEvent } from "../events";
import { issueLicense } from "../issue";

/** Dodo Payments webhooks (Standard Webhooks signatures), verified before
 * anything is read, then parsed into typed events (src/events.ts). Dodo only
 * takes payments: a paid order gets a license key dotsafe issues and emails
 * itself, and a full refund revokes it. Anything that fails answers 500, so
 * Dodo retries the delivery; every step is safe to repeat. */
export const webhooks = new Hono<AppEnv>();

webhooks.post("/dodo", async (c) => {
  // Verify against the raw body exactly as sent; parsing first can change it.
  const raw = await c.req.text();
  const id = c.req.header("webhook-id") ?? "";
  let body: { type?: unknown };
  try {
    new Webhook(c.env.DODO_WEBHOOK_KEY).verify(raw, {
      "webhook-id": id,
      "webhook-timestamp": c.req.header("webhook-timestamp") ?? "",
      "webhook-signature": c.req.header("webhook-signature") ?? "",
    });
    body = JSON.parse(raw);
  } catch {
    return c.json({ error: { code: "invalid_signature", message: "Invalid webhook signature." } }, 401);
  }

  const db = records(c.env.DB);
  if (await db.seen(id)) return c.json({ received: true });

  // Never log a license key, email or name: ids only.
  const handled = await handle(c.env, parseEvent(body));
  console.log(JSON.stringify({ webhook: body.type, id, ...handled }));
  await db.handled(id, String(body.type));
  return c.json({ received: true });
});

async function handle(env: Env, event: DotsafeEvent): Promise<Record<string, unknown>> {
  const db = records(env.DB);
  switch (event.kind) {
    case "payment": {
      if (!event.productIds.includes(env.DODO_PRODUCT_ID)) return { payment_id: event.paymentId, ignored: "not dotsafe" };
      await db.savePurchase({
        paymentId: event.paymentId,
        customerId: event.customerId,
        email: event.email,
        name: event.name,
        amount: event.amount,
        currency: event.currency,
        paidAt: event.at,
      });
      const issued = await issueLicense(env, { email: event.email, name: event.name, source: "purchase", paymentId: event.paymentId });
      return { payment_id: event.paymentId, license_id: issued.license.id, emailed: issued.emailed };
    }

    case "refund": {
      const full = await db.refund(event.paymentId, event.amount, event.at);
      // A partial refund keeps the license; a full one ends it.
      if (full) await db.revokeForPayment(event.paymentId, "refund", event.at);
      return { payment_id: event.paymentId, refund_id: event.refundId, revoked: full };
    }

    case "dispute":
      await db.dispute(event.paymentId, event.at);
      return { payment_id: event.paymentId, dispute_id: event.disputeId };

    case "malformed":
      return { ignored: "malformed" };

    case "other":
      return { ignored: true };
  }
}
