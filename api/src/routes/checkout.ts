import { Hono } from "hono";
import type { AppEnv } from "../env";
import { dodo } from "../dodo";
import { fail } from "../errors";
import { readObject, str } from "../validate";

/** Starts a purchase from the website: returns a hosted Dodo checkout URL. */
export const checkout = new Hono<AppEnv>();

checkout.post("/", async (c) => {
  if (!c.env.DODO_PRODUCT_ID || !c.env.DODO_API_KEY) {
    return fail(c, 503, "not_configured", "Checkout isn't available right now. Try again later.");
  }
  const body = (await readObject(c)) ?? {};
  const email = str(body.email, 254) ?? undefined;
  try {
    const session = await dodo(c.env).createCheckout({
      productId: c.env.DODO_PRODUCT_ID,
      returnUrl: c.env.CHECKOUT_RETURN_URL,
      email,
    });
    if (!session.checkout_url) throw new Error("no checkout url");
    return c.json({ checkout_url: session.checkout_url });
  } catch {
    return fail(c, 502, "upstream_error", "Couldn't start checkout. Try again in a moment.");
  }
});
