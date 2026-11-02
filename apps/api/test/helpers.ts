import { vi } from "vitest";
import type { Env } from "../src/env";

export const env: Env = {
  DODO_MODE: "test_mode",
  DODO_PRODUCT_ID: "pdt_dotsafe",
  CHECKOUT_RETURN_URL: "https://dotsafe.dev/thanks",
  ALLOWED_ORIGINS: "https://dotsafe.dev",
  DODO_API_KEY: "dodo_test_secret",
  // base64 of "test-webhook-signing-key-0123456789"
  DODO_WEBHOOK_KEY: "whsec_dGVzdC13ZWJob29rLXNpZ25pbmcta2V5LTAxMjM0NTY3ODk=",
};

type Call = { url: string; method: string; headers: Record<string, string>; body: unknown };

/** Replaces fetch with scripted Dodo responses, in order, and records each call. */
export function mockDodo(...responses: Array<{ status: number; body?: unknown } | Error>) {
  const calls: Call[] = [];
  const queue = [...responses];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string, init: RequestInit = {}) => {
      calls.push({
        url,
        method: init.method ?? "GET",
        headers: Object.fromEntries(Object.entries((init.headers ?? {}) as Record<string, string>)),
        body: init.body ? JSON.parse(init.body as string) : undefined,
      });
      const next = queue.shift();
      if (!next) throw new Error(`unexpected fetch to ${url}`);
      if (next instanceof Error) throw next;
      return new Response(next.body === undefined ? "" : JSON.stringify(next.body), { status: next.status });
    }),
  );
  return calls;
}

export function post(path: string, body: unknown, headers: Record<string, string> = {}) {
  return new Request(`http://api.test${path}`, {
    method: "POST",
    headers: { "content-type": "application/json", ...headers },
    body: typeof body === "string" ? body : JSON.stringify(body),
  });
}
