// The records in D1 (schema: migrations/): purchases, the license keys
// dotsafe issues, and the devices they're activated on. Webhook writes are
// upserts, so a delivery the provider retries, or sends out of order, lands
// the same way twice. Keys are stored only as hashes (src/keys.ts).

export type Purchase = {
  paymentId: string;
  customerId: string;
  email: string;
  name: string | null;
  amount: number;
  currency: string;
  paidAt: string;
};

export type LicenseRow = {
  id: string;
  key_hint: string;
  email: string;
  name: string | null;
  source: "purchase" | "manual";
  payment_id: string | null;
  activations_limit: number;
  emailed_at: string | null;
  revoked_at: string | null;
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
    },

    /** Adds a refund and returns whether the payment is now fully refunded. */
    async refund(paymentId: string, amount: number | null, at: string) {
      await db
        .prepare(
          "UPDATE purchases SET refunded_amount = COALESCE(refunded_amount, 0) + COALESCE(?, amount), refunded_at = ? WHERE payment_id = ?",
        )
        .bind(amount, at, paymentId)
        .run();
      const row = await db
        .prepare("SELECT refunded_amount >= amount AS full FROM purchases WHERE payment_id = ?")
        .bind(paymentId)
        .first<{ full: number }>();
      return row?.full === 1;
    },

    async dispute(paymentId: string, at: string) {
      await db.prepare("UPDATE purchases SET disputed_at = ? WHERE payment_id = ?").bind(at, paymentId).run();
    },

    /** Creates a license. For a purchase, at most one per payment: a second
     * call returns the existing one. */
    async createLicense(l: {
      id: string;
      keyHash: string;
      keyHint: string;
      email: string;
      name: string | null;
      source: "purchase" | "manual";
      paymentId: string | null;
      activationsLimit: number;
    }): Promise<LicenseRow> {
      await db
        .prepare(
          `INSERT INTO licenses (id, key_hash, key_hint, email, name, source, payment_id, activations_limit, created_at)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
           ON CONFLICT (payment_id) DO NOTHING`,
        )
        .bind(l.id, l.keyHash, l.keyHint, l.email, l.name, l.source, l.paymentId, l.activationsLimit, now())
        .run();
      const row = l.paymentId
        ? await db.prepare("SELECT * FROM licenses WHERE payment_id = ?").bind(l.paymentId).first<LicenseRow>()
        : await db.prepare("SELECT * FROM licenses WHERE id = ?").bind(l.id).first<LicenseRow>();
      return row!;
    },

    async licenseByHash(keyHash: string) {
      return db.prepare("SELECT * FROM licenses WHERE key_hash = ?").bind(keyHash).first<LicenseRow>();
    },

    /** Claims the license email and sets the key it will carry, in one
     * statement: the email goes out once even if two deliveries race, and the
     * stored hash is always the key that was sent. Replacing the key is safe
     * because an unclaimed license's key never reached anyone. False if the
     * email was already sent (or is being sent). */
    async claimEmail(licenseId: string, keyHash: string, keyHint: string) {
      const res = await db
        .prepare("UPDATE licenses SET emailed_at = ?, key_hash = ?, key_hint = ? WHERE id = ? AND emailed_at IS NULL")
        .bind(now(), keyHash, keyHint, licenseId)
        .run();
      return res.meta.changes > 0;
    },

    /** Gives the claim back after a failed send, so a retry tries again. */
    async releaseEmail(licenseId: string) {
      await db.prepare("UPDATE licenses SET emailed_at = NULL WHERE id = ?").bind(licenseId).run();
    },

    async revokeForPayment(paymentId: string, reason: string, at: string) {
      await db
        .prepare("UPDATE licenses SET revoked_at = ?, revoked_reason = ? WHERE payment_id = ? AND revoked_at IS NULL")
        .bind(at, reason, paymentId)
        .run();
    },

    /** Adds a device if the license has a free slot, in one statement so two
     * activations at once can't both take the last slot. False when full. */
    async activate(a: { instanceId: string; licenseId: string; deviceName: string; limit: number; at: string }) {
      const res = await db
        .prepare(
          `INSERT INTO activations (instance_id, license_id, device_name, activated_at, last_seen_at)
           SELECT ?, ?, ?, ?, ?
           WHERE (SELECT count(*) FROM activations WHERE license_id = ? AND deactivated_at IS NULL) < ?`,
        )
        .bind(a.instanceId, a.licenseId, a.deviceName, a.at, a.at, a.licenseId, a.limit)
        .run();
      return res.meta.changes > 0;
    },

    async activeDevices(licenseId: string) {
      const row = await db
        .prepare("SELECT count(*) AS n FROM activations WHERE license_id = ? AND deactivated_at IS NULL")
        .bind(licenseId)
        .first<{ n: number }>();
      return row?.n ?? 0;
    },

    /** A license's still-active device, or null. */
    async activeDevice(licenseId: string, instanceId: string) {
      return db
        .prepare("SELECT instance_id FROM activations WHERE instance_id = ? AND license_id = ? AND deactivated_at IS NULL")
        .bind(instanceId, licenseId)
        .first<{ instance_id: string }>();
    },

    async seenDevice(instanceId: string) {
      await db.prepare("UPDATE activations SET last_seen_at = ? WHERE instance_id = ?").bind(now(), instanceId).run();
    },

    async deactivate(licenseId: string, instanceId: string) {
      await db
        .prepare("UPDATE activations SET deactivated_at = ? WHERE instance_id = ? AND license_id = ? AND deactivated_at IS NULL")
        .bind(now(), instanceId, licenseId)
        .run();
    },
  };
}
