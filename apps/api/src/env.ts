/** Worker bindings. Vars live in wrangler.jsonc; secrets are set with `wrangler secret put`. */
export type Env = {
  /** "test_mode" or "live_mode": which Dodo Payments environment to call. */
  DODO_MODE: "test_mode" | "live_mode";
  /** The dotsafe product in Dodo; license keys for any other product are rejected. */
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
  /** Purchases, licenses and activations (migrations/). */
  DB: D1Database;
  /** Cloudflare Email Service, for the license email. */
  EMAIL: SendEmail;
};

export type AppEnv = { Bindings: Env };
