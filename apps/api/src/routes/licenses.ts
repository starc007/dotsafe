import { Hono } from "hono";
import type { AppEnv } from "../env";
import { DodoError, dodo } from "../dodo";
import { fail } from "../errors";
import { readObject, str } from "../validate";

// License activation for the desktop app and CLI. The app only ever talks to
// these endpoints; the product check needs the secret API key, so it can't
// happen on the user's machine.

export const licenses = new Hono<AppEnv>();

const MAX_KEY = 128;
const MAX_DEVICE = 100;

licenses.post("/activate", async (c) => {
  const body = await readObject(c);
  const licenseKey = str(body?.license_key, MAX_KEY);
  const deviceName = str(body?.device_name, MAX_DEVICE) ?? "dotsafe device";
  if (!licenseKey) return fail(c, 400, "invalid_request", "Enter your license key.");
  if (!c.env.DODO_PRODUCT_ID || !c.env.DODO_API_KEY) {
    return fail(c, 503, "not_configured", "Licensing isn't available right now. Try again later.");
  }

  const client = dodo(c.env);
  let instance;
  try {
    instance = await client.activate(licenseKey, deviceName);
  } catch (e) {
    if (e instanceof DodoError) {
      if (e.status === 404) return fail(c, 404, "not_found", "We couldn't find that license key. Check it and try again.");
      if (e.status === 403) {
        return fail(c, 403, "inactive", "This license key isn't active. It may have been refunded or disabled.");
      }
      if (e.status === 422) {
        return fail(
          c,
          409,
          "limit_reached",
          "This key is already in use on the maximum number of devices. Deactivate it on another device first.",
        );
      }
    }
    return fail(c, 502, "upstream_error", "Couldn't reach the license server. Try again in a moment.");
  }

  // Dodo's public activation accepts any of its merchants' keys, so confirm
  // this one is for dotsafe. If not, release the activation we just made.
  let key;
  try {
    key = await client.getLicenseKey(instance.license_key_id);
  } catch {
    await client.deactivate(licenseKey, instance.id).catch(() => {});
    return fail(c, 502, "upstream_error", "Couldn't confirm the license. Try again in a moment.");
  }
  if (key.product_id !== c.env.DODO_PRODUCT_ID) {
    await client.deactivate(licenseKey, instance.id).catch(() => {});
    return fail(c, 403, "wrong_product", "This license key isn't for dotsafe.");
  }

  return c.json(
    {
      instance_id: instance.id,
      activated_at: instance.created_at,
      activations: { used: key.instances_count, limit: key.activations_limit },
      expires_at: key.expires_at,
    },
    201,
  );
});

licenses.post("/validate", async (c) => {
  const body = await readObject(c);
  const licenseKey = str(body?.license_key, MAX_KEY);
  const instanceId = str(body?.instance_id, MAX_KEY);
  if (!licenseKey || !instanceId) return fail(c, 400, "invalid_request", "A license key and instance id are required.");
  try {
    const { valid } = await dodo(c.env).validate(licenseKey, instanceId);
    return c.json({ valid });
  } catch (e) {
    if (e instanceof DodoError && e.status === 422) {
      return fail(c, 400, "invalid_request", "That isn't a valid license key.");
    }
    return fail(c, 502, "upstream_error", "Couldn't reach the license server. Try again in a moment.");
  }
});

licenses.post("/deactivate", async (c) => {
  const body = await readObject(c);
  const licenseKey = str(body?.license_key, MAX_KEY);
  const instanceId = str(body?.instance_id, MAX_KEY);
  if (!licenseKey || !instanceId) return fail(c, 400, "invalid_request", "A license key and instance id are required.");
  try {
    await dodo(c.env).deactivate(licenseKey, instanceId);
    return c.json({ ok: true });
  } catch (e) {
    // Already gone (instance or key not found): the slot is free either way.
    if (e instanceof DodoError && (e.status === 403 || e.status === 404)) return c.json({ ok: true });
    return fail(c, 502, "upstream_error", "Couldn't reach the license server. Try again in a moment.");
  }
});
