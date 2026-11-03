/// <reference types="node" />
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { vi } from "vitest";
import type { Env } from "../src/env";

/** A D1 stand-in on Node's SQLite, with every migration applied, so tests run
 * the real schema and SQL. Covers the calls src/db.ts makes. */
export function fakeD1() {
  const sqlite = new DatabaseSync(":memory:");
  const dir = join(import.meta.dirname, "../migrations");
  for (const file of readdirSync(dir).sort()) sqlite.exec(readFileSync(join(dir, file), "utf8"));
  const prepare = (sql: string) => {
    let args: any[] = [];
    const stmt = {
      bind: (...values: any[]) => ((args = values), stmt),
      run: async () => ({ success: true, meta: { changes: Number(sqlite.prepare(sql).run(...args).changes) } }),
      first: async () => sqlite.prepare(sql).get(...args) ?? null,
      all: async () => ({ success: true, results: sqlite.prepare(sql).all(...args), meta: {} }),
    };
    return stmt;
  };
  return { db: { prepare } as unknown as D1Database, sqlite };
}

/** An Email Service binding that keeps what it was asked to send. */
export function fakeMailer() {
  const sent: any[] = [];
  return { binding: { send: async (m: any) => (sent.push(m), { messageId: `m_${sent.length}` }) } as unknown as SendEmail, sent };
}

/** A fresh env: its own database and outbox. */
export function testEnv() {
  const d1 = fakeD1();
  const mail = fakeMailer();
  return { env: { ...base, DB: d1.db, EMAIL: mail.binding } as Env, sqlite: d1.sqlite, sent: mail.sent };
}

const base = {
  DODO_MODE: "test_mode",
  DODO_PRODUCT_ID: "pdt_dotsafe",
  CHECKOUT_RETURN_URL: "https://dotsafe.dev/thanks",
  ALLOWED_ORIGINS: "https://dotsafe.dev",
  DODO_API_KEY: "dodo_test_secret",
  // base64 of "test-webhook-signing-key-0123456789"
  DODO_WEBHOOK_KEY: "whsec_dGVzdC13ZWJob29rLXNpZ25pbmcta2V5LTAxMjM0NTY3ODk=",
  EMAIL_FROM: "hi@dotsafe.dev",
  EMAIL_REPLY_TO: "",
};

export const env: Env = testEnv().env;

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
