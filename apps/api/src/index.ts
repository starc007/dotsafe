import { Hono } from "hono";
import { cors } from "hono/cors";
import type { AppEnv } from "./env";
import { rateLimit, smallBody, webhookBody } from "./limits";
import { admin } from "./routes/admin";
import { checkout } from "./routes/checkout";
import { licenses } from "./routes/licenses";
import { webhooks } from "./routes/webhooks";

const app = new Hono<AppEnv>();

// Only the website calls the API from a browser (checkout). The desktop app
// and CLI make native requests, which CORS doesn't apply to.
app.use("/v1/checkout", async (c, next) => {
  const origins = c.env.ALLOWED_ORIGINS.split(",").map((o) => o.trim()).filter(Boolean);
  return cors({ origin: origins, allowMethods: ["POST"], allowHeaders: ["content-type"] })(c, next);
});

// Size caps first, then per-IP limits. Webhooks aren't rate limited: they're
// signed by Dodo, and limiting them could drop a real purchase.
app.use("/v1/licenses/*", smallBody, rateLimit("LICENSE_LIMITER", "licenses"));
app.use("/v1/checkout", smallBody, rateLimit("CHECKOUT_LIMITER", "checkout"));
app.use("/v1/webhooks/*", webhookBody);
app.use("/v1/admin/*", smallBody, rateLimit("LICENSE_LIMITER", "admin"));

app.get("/health", (c) => c.json({ ok: true }));
app.route("/v1/licenses", licenses);
app.route("/v1/admin", admin);
app.route("/v1/checkout", checkout);
app.route("/v1/webhooks", webhooks);

app.notFound((c) => c.json({ error: { code: "not_found", message: "Not found." } }, 404));
app.onError((err, c) => {
  console.error(err);
  return c.json({ error: { code: "internal_error", message: "Something went wrong." } }, 500);
});

export default app;
