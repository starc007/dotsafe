// The purchase records in D1 (schema: migrations/). Writes are upserts so a
// webhook Dodo retries, or delivers out of order, lands the same way twice.

export type Purchase = {
  paymentId: string;
  customerId: string;
  email: string;
  name: string | null;
  amount: number;
  currency: string;
  paidAt: string;
};

export type License = {
  id: string;
  key: string;
  paymentId: string | null;
  customerId: string;
  email: string | null;
  activationsLimit: number | null;
  createdAt: string;
  expiresAt: string | null;
};

const now = () => new Date().toISOString();

export function records(db: D1Database) {
  return {
    /** True if this webhook was handled before. */
    async seen(webhookId: string) {
      return (await db.prepare("SELECT 1 FROM webhook_events WHERE id = ?").bind(webhookId).first()) !== null;
    },

    /** Marks a webhook handled, once everything it asked for is done. */
    async handled(webhookId: string, type: string) {
      await db
        .prepare("INSERT OR IGNORE INTO webhook_events (id, type, received_at) VALUES (?, ?, ?)")
        .bind(webhookId, type, now())
        .run();
    },

    async savePurchase(p: Purchase) {
      await db
        .prepare(
          `INSERT INTO purchases (payment_id, customer_id, email, name, amount, currency, paid_at)
           VALUES (?, ?, ?, ?, ?, ?, ?)
           ON CONFLICT (payment_id) DO UPDATE SET
             customer_id = excluded.customer_id, email = excluded.email, name = excluded.name,
             amount = excluded.amount, currency = excluded.currency, paid_at = excluded.paid_at`,
        )
        .bind(p.paymentId, p.customerId, p.email, p.name, p.amount, p.currency, p.paidAt)
        .run();
      // A license that arrived first has no email yet; fill it in.
      await db
        .prepare("UPDATE licenses SET email = ? WHERE payment_id = ? AND email IS NULL")
        .bind(p.email, p.paymentId)
        .run();
    },

    async purchase(paymentId: string) {
      return db
        .prepare("SELECT email, name FROM purchases WHERE payment_id = ?")
        .bind(paymentId)
        .first<{ email: string; name: string | null }>();
    },

    async refund(paymentId: string, amount: number | null, at: string) {
      await db
        .prepare(
          "UPDATE purchases SET refunded_amount = COALESCE(refunded_amount, 0) + COALESCE(?, amount), refunded_at = ? WHERE payment_id = ?",
        )
        .bind(amount, at, paymentId)
        .run();
    },

    async dispute(paymentId: string, at: string) {
      await db.prepare("UPDATE purchases SET disputed_at = ? WHERE payment_id = ?").bind(at, paymentId).run();
    },

    async saveLicense(l: License) {
      await db
        .prepare(
          `INSERT INTO licenses (id, key, payment_id, customer_id, email, activations_limit, created_at, expires_at)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?)
           ON CONFLICT (id) DO UPDATE SET
             payment_id = COALESCE(excluded.payment_id, payment_id),
             email = COALESCE(excluded.email, email),
             activations_limit = excluded.activations_limit, expires_at = excluded.expires_at`,
        )
        .bind(l.id, l.key, l.paymentId, l.customerId, l.email, l.activationsLimit, l.createdAt, l.expiresAt)
        .run();
    },

    async revokeLicense(id: string, reason: string | null, at: string) {
      await db
        .prepare("UPDATE licenses SET revoked_at = ?, revoked_reason = ? WHERE id = ?")
        .bind(at, reason, id)
        .run();
    },

    /** Claims the license email so it goes out once, even if two deliveries race.
     * False if it was already sent (or is being sent). */
    async claimEmail(licenseId: string) {
      const res = await db
        .prepare("UPDATE licenses SET emailed_at = ? WHERE id = ? AND emailed_at IS NULL")
        .bind(now(), licenseId)
        .run();
      return res.meta.changes > 0;
    },

    /** Gives the claim back after a failed send, so Dodo's retry tries again. */
    async releaseEmail(licenseId: string) {
      await db.prepare("UPDATE licenses SET emailed_at = NULL WHERE id = ?").bind(licenseId).run();
    },

    async activated(a: { instanceId: string; licenseId: string; deviceName: string; at: string }) {
      await db
        .prepare(
          `INSERT INTO activations (instance_id, license_id, device_name, activated_at, last_seen_at)
           VALUES (?, ?, ?, ?, ?)
           ON CONFLICT (instance_id) DO NOTHING`,
        )
        .bind(a.instanceId, a.licenseId, a.deviceName, a.at, a.at)
        .run();
    },

    async seenDevice(instanceId: string) {
      await db.prepare("UPDATE activations SET last_seen_at = ? WHERE instance_id = ?").bind(now(), instanceId).run();
    },

    async deactivated(instanceId: string) {
      await db
        .prepare("UPDATE activations SET deactivated_at = ? WHERE instance_id = ? AND deactivated_at IS NULL")
        .bind(now(), instanceId)
        .run();
    },
  };
}

/** Runs a record write that must never fail the request it rides on: the
 * app's license calls work even if D1 is down or not set up. */
export async function bestEffort(what: string, write: () => Promise<unknown>) {
  try {
    await write();
  } catch (e) {
    console.error(JSON.stringify({ record_failed: what, error: String(e) }));
  }
}
