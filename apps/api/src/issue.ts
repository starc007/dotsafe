import type { Env } from "./env";
import { records } from "./db";
import { sendLicenseEmail } from "./email";
import { hashKey, keyHint, newId, newKey } from "./keys";

/** Macs per license, as sold on the pricing page. */
export const DEVICE_LIMIT = 2;

/** Issues a license and emails its key, once. Used for purchases (one per
 * payment, safe to call again on a retried webhook) and for keys given away
 * with license:create. Only the key's hash is stored, so the key exists only
 * in this call and the email. Whoever claims the email sets the key it
 * carries; if sending fails, the claim is released and a retry sends a new
 * key, since nobody received the old one. */
export async function issueLicense(
  env: Env,
  args: { email: string; name: string | null; source: "purchase" | "manual"; paymentId: string | null },
) {
  const db = records(env.DB);
  const key = newKey();
  const keyHash = await hashKey(key);
  const license = await db.createLicense({
    id: newId("lic"),
    keyHash,
    keyHint: keyHint(key),
    email: args.email,
    name: args.name,
    source: args.source,
    paymentId: args.paymentId,
    activationsLimit: DEVICE_LIMIT,
  });

  if (!(await db.claimEmail(license.id, keyHash, keyHint(key)))) return { license, key: null, emailed: "already" as const };
  try {
    await sendLicenseEmail(env, args.email, { key, name: args.name, limit: DEVICE_LIMIT });
  } catch (e) {
    await db.releaseEmail(license.id);
    throw e;
  }
  return { license: { ...license, key_hint: keyHint(key) }, key, emailed: true as const };
}
