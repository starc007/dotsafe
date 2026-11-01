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
  /** Secret: Dodo API key. Server-side only, never shipped in the app. */
  DODO_API_KEY: string;
  /** Secret: Dodo webhook signing key (whsec_…). */
  DODO_WEBHOOK_KEY: string;
};

export type AppEnv = { Bindings: Env };
