import { Hono } from "hono";
import { Webhook } from "standardwebhooks";
import type { AppEnv, Env } from "../env";
import { records } from "../db";
import { dodo } from "../dodo";
import { sendLicenseEmail } from "../email";

/** Dodo Payments webhooks (Standard Webhooks signatures). Verified before
 * anything is read, then recorded in D1. A new license key is emailed from
 * here. Anything that fails answers 500, so Dodo retries the delivery; every
 * step is safe to repeat. */
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

  // Never log a full license key, email or name: ids only.
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
      if (forDotsafe === false) return { payment_id: d.payment_id, ignored: "other product" };
      await db.savePurchase({
        paymentId: d.payment_id,
        customerId: d.customer.customer_id,
        email: d.customer.email,
        name: d.customer.name ?? null,
        amount: d.total_amount,
        currency: d.currency,
        paidAt: at,
      });
      return { payment_id: d.payment_id };
    }

    case "refund.succeeded":
      await db.refund(d.payment_id, typeof d.amount === "number" ? d.amount : null, at);
      return { payment_id: d.payment_id, refund_id: d.refund_id };

    case "dispute.opened":
      await db.dispute(d.payment_id, at);
      return { payment_id: d.payment_id, dispute_id: d.dispute_id };

    // An automatically issued key arrives as `created` with status Delivered;
    // a manually fulfilled one as `delivered`. Either way, record and email it.
    case "entitlement_grant.created":
    case "entitlement_grant.delivered": {
      const key = d.license_key as { id: string; key: string; activations_limit: number | null; expires_at: string | null } | null;
      if (d.integration_type !== "license_key" || !key) return { grant_id: d.id, ignored: "no license key" };
      return { grant_id: d.id, license_id: key.id, ...(await deliverLicense(env, d, key)) };
    }

    case "entitlement_grant.revoked":
      if (d.license_key?.id) await db.revokeLicense(d.license_key.id, d.revocation_reason ?? null, d.revoked_at ?? at);
      return { grant_id: d.id, license_id: d.license_key?.id };

    default:
      return { ignored: true };
  }
}

/** Records a new license key and emails it to the buyer, once. */
async function deliverLicense(
  env: Env,
  grant: Record<string, any>,
  key: { id: string; key: string; activations_limit: number | null; expires_at: string | null },
) {
  const db = records(env.DB);
  const client = dodo(env);

  // One Dodo account could sell more than dotsafe: only dotsafe keys are ours.
  const details = await client.getLicenseKey(key.id);
  if (details.product_id !== env.DODO_PRODUCT_ID) return { ignored: "other product" };

  // The payment webhook may not have arrived yet; ask Dodo for the buyer then.
  const paymentId = (grant.payment_id as string | null) ?? null;
  const known = paymentId ? await db.purchase(paymentId) : null;
  const buyer = known ?? (await client.getCustomer(grant.customer_id));

  await db.saveLicense({
    id: key.id,
    key: key.key,
    paymentId,
    customerId: grant.customer_id,
    email: buyer.email,
    activationsLimit: key.activations_limit,
    createdAt: grant.created_at ?? new Date().toISOString(),
    expiresAt: key.expires_at,
  });

  if (!(await db.claimEmail(key.id))) return { emailed: "already" };
  try {
    await sendLicenseEmail(env, buyer.email, { key: key.key, name: buyer.name, limit: key.activations_limit });
  } catch (e) {
    await db.releaseEmail(key.id);
    throw e;
  }
  return { emailed: true };
}
