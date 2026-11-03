import type { Context } from "hono";
import type { ContentfulStatusCode } from "hono/utils/http-status";

/** Stable error codes the app and website can branch on. Messages are user-facing. */
export type ErrorCode =
  | "invalid_request"
  | "not_found"
  | "inactive"
  | "limit_reached"
  | "not_configured"
  | "upstream_error"
  | "rate_limited"
  | "too_large"
  | "unauthorized";

export function fail(c: Context, status: ContentfulStatusCode, code: ErrorCode, message: string) {
  return c.json({ error: { code, message } }, status);
}
