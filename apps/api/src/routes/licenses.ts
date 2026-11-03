import { Hono, type Context } from "hono";
import type { AppEnv } from "../env";
import { type LicenseRow, records } from "../db";
import { fail } from "../errors";
import { hashKey, newId } from "../keys";
import { readObject, str } from "../validate";

// License activation for the desktop app and CLI, against the keys dotsafe
// issues (src/issue.ts). Nothing here calls the payment provider, so changing
// providers never touches a key. Every lookup is by the key's hash; a key
// that's wrong, made up or revoked never changes a record.

export const licenses = new Hono<AppEnv>();

const MAX_KEY = 128;
const MAX_DEVICE = 100;

/** The request's body, its instance id if any, and the license its key
 * belongs to (null for an unknown key). Null when there's no usable key. */
async function keyRequest(c: Context<AppEnv>) {
  const body = await readObject(c);
  const licenseKey = str(body?.license_key, MAX_KEY);
  if (!licenseKey) return null;
  const license: LicenseRow | null = await records(c.env.DB).licenseByHash(await hashKey(licenseKey));
  return { body, instanceId: str(body?.instance_id, MAX_KEY), license };
}

licenses.post("/activate", async (c) => {
  const req = await keyRequest(c);
  if (!req) return fail(c, 400, "invalid_request", "Enter your license key.");
  const { license } = req;
  if (!license) return fail(c, 404, "not_found", "We couldn't find that license key. Check it and try again.");
  if (license.revoked_at) {
    return fail(c, 403, "inactive", "This license key isn't active. It may have been refunded or disabled.");
  }

  const db = records(c.env.DB);
  const instanceId = newId("act");
  const at = new Date().toISOString();
  const deviceName = str(req.body?.device_name, MAX_DEVICE) ?? "dotsafe device";
  if (!(await db.activate({ instanceId, licenseId: license.id, deviceName, limit: license.activations_limit, at }))) {
    return fail(
      c,
      409,
      "limit_reached",
      "This key is already in use on the maximum number of devices. Deactivate it on another device first.",
    );
  }
  return c.json(
    {
      instance_id: instanceId,
      activated_at: at,
      activations: { used: await db.activeDevices(license.id), limit: license.activations_limit },
      expires_at: null,
    },
    201,
  );
});

licenses.post("/validate", async (c) => {
  const req = await keyRequest(c);
  if (!req?.instanceId) return fail(c, 400, "invalid_request", "A license key and instance id are required.");
  const { license, instanceId } = req;
  const db = records(c.env.DB);
  // Valid: the key exists, isn't revoked, and this device is still one of its activations.
  const valid = !!license && !license.revoked_at && !!(await db.activeDevice(license.id, instanceId));
  if (valid) await db.seenDevice(instanceId);
  return c.json({ valid });
});

licenses.post("/deactivate", async (c) => {
  const req = await keyRequest(c);
  if (!req?.instanceId) return fail(c, 400, "invalid_request", "A license key and instance id are required.");
  // Only the key's own device can be freed. The answer is the same either way,
  // so it doesn't say whether a key or device exists.
  if (req.license) await records(c.env.DB).deactivate(req.license.id, req.instanceId);
  return c.json({ ok: true });
});
