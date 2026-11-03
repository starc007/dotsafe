import { Hono } from "hono";
import { Webhook } from "standardwebhooks";
import type { AppEnv, Env } from "../env";
import { records } from "../db";
import { issueLicense } from "../issue";

/** Dodo Payments webhooks (Standard Webhooks signatures), verified before
 * anything is read. Dodo only takes payments: a paid order gets a license key
 * dotsafe issues and emails itself, and a full refund revokes it. Anything that
 * fails answers 500, so Dodo retries the delivery; every step is safe to repeat. */
export const webhooks = new Hono<AppEnv>();

type DodoEvent = { type: string; timestamp?: string; data?: Record<string, any> };

webhooks.post("/dodo", async (c) => {
  // Verify against the raw body exactly as sent; parsing first can change it.
  const raw = await c.req.text();
  const id = c.req.header("webhook-id") ?? "";
  let event: DodoEvent;
  try {
    new Webhook(c.env.DODO_WEBHOOK_KEY).verify(raw, {
      "webhook-id": id,
      "webhook-timestamp": c.req.header("webhook-timestamp") ?? "",
      "webhook-signature": c.req.header("webhook-signature") ?? "",
    });
    event = JSON.parse(raw) as DodoEvent;
  } catch {
    return c.json({ error: { code: "invalid_signature", message: "Invalid webhook signature." } }, 401);
  }

  const db = records(c.env.DB);
  if (await db.seen(id)) return c.json({ received: true });

  // Never log a license key, email or name: ids only.
  const handled = await handle(c.env, event);
  console.log(JSON.stringify({ webhook: event.type, id, ...handled }));
  await db.handled(id, event.type);
  return c.json({ received: true });
});

async function handle(env: Env, event: DodoEvent): Promise<Record<string, unknown>> {
  const db = records(env.DB);
  const d = event.data ?? {};
  const at = (d.created_at as string | undefined) ?? event.timestamp ?? new Date().toISOString();

  switch (event.type) {
    case "payment.succeeded": {
      const forDotsafe = (d.product_cart as { product_id: string }[] | null | undefined)?.some(
        (item) => item.product_id === env.DODO_PRODUCT_ID,
      );
      if (!forDotsafe) return { payment_id: d.payment_id, ignored: "not dotsafe" };
      await db.savePurchase({
        paymentId: d.payment_id,
        customerId: d.customer.customer_id,
        email: d.customer.email,
        name: d.customer.name ?? null,
        amount: d.total_amount,
        currency: d.currency,
        paidAt: at,
      });
      const issued = await issueLicense(env, {
        email: d.customer.email,
        name: d.customer.name ?? null,
        source: "purchase",
        paymentId: d.payment_id,
      });
      return { payment_id: d.payment_id, license_id: issued.license.id, emailed: issued.emailed };
    }

    case "refund.succeeded": {
      const full = await db.refund(d.payment_id, typeof d.amount === "number" ? d.amount : null, at);
      // A partial refund keeps the license; a full one ends it.
      if (full) await db.revokeForPayment(d.payment_id, "refund", at);
      return { payment_id: d.payment_id, refund_id: d.refund_id, revoked: full };
    }

    case "dispute.opened":
      await db.dispute(d.payment_id, at);
      return { payment_id: d.payment_id, dispute_id: d.dispute_id };

    default:
      return { ignored: true };
  }
}
