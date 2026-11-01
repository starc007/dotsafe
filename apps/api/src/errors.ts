import type { Context } from "hono";
import type { ContentfulStatusCode } from "hono/utils/http-status";

/** Stable error codes the app and website can branch on. Messages are user-facing. */
export type ErrorCode =
  | "invalid_request"
  | "not_found"
  | "inactive"
  | "limit_reached"
  | "wrong_product"
  | "not_configured"
  | "upstream_error";

export function fail(c: Context, status: ContentfulStatusCode, code: ErrorCode, message: string) {
  return c.json({ error: { code, message } }, status);
}
