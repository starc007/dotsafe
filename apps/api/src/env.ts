/** Worker bindings. Vars live in wrangler.jsonc; secrets are set with `wrangler secret put`. */
export type Env = {
  /** "test_mode" or "live_mode": which Dodo Payments environment to call. */
  DODO_MODE: "test_mode" | "live_mode";
  /** The dotsafe product in Dodo: checkouts sell it, and only payments for it issue a license. */
  DODO_PRODUCT_ID: string;
  /** Where Dodo sends customers after checkout. */
  CHECKOUT_RETURN_URL: string;
  /** Comma-separated origins allowed to call the API from a browser (the website). */
  ALLOWED_ORIGINS: string;
  /** Who license emails come from. Its domain must be set up for Cloudflare Email Sending. */
  EMAIL_FROM: string;
  /** Where replies to license emails go. Empty: no reply-to, and the email doesn't invite replies. */
  EMAIL_REPLY_TO: string;
  /** Secret: Dodo API key. Server-side only, never shipped in the app. */
  DODO_API_KEY: string;
  /** Secret: Dodo webhook signing key (whsec_…). */
  DODO_WEBHOOK_KEY: string;
  /** Secret: bearer token for /v1/admin (license:create). Unset turns the admin routes off. */
  ADMIN_TOKEN?: string;
  /** Purchases, licenses and activations (migrations/). */
  DB: D1Database;
  /** Cloudflare Email Service, for the license email. */
  EMAIL: SendEmail;
  /** Per-IP limit on the license routes (src/limits.ts). Optional so tests can leave it out. */
  LICENSE_LIMITER?: RateLimit;
  /** Per-IP limit on starting checkouts. */
  CHECKOUT_LIMITER?: RateLimit;
};

export type AppEnv = { Bindings: Env };
