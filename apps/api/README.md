# dotsafe API

A small Cloudflare Worker (Hono) for dotsafe's licenses. dotsafe issues and checks its own license keys; [Dodo Payments](https://dodopayments.com) only takes the payment. The Worker handles license activation for the desktop app and CLI, checkout for the website, and Dodo's payment webhooks. It keeps purchases, license keys and activated devices in D1, and emails each new key. It never sees vaults or secrets.

| Route | Used by | What it does |
| --- | --- | --- |
| `POST /v1/licenses/activate` `{license_key, device_name}` | app | Activates the key on a device if it has a free slot. Returns `instance_id`. |
| `POST /v1/licenses/validate` `{license_key, instance_id}` | app | `{valid}`: the key exists, isn't revoked, and the device is still activated. |
| `POST /v1/licenses/deactivate` `{license_key, instance_id}` | app | Frees the device's slot. Always `{ok: true}`, so it doesn't say what exists. |
| `POST /v1/checkout` `{email?}` | website | `{checkout_url}` for the dotsafe product. CORS limited to `ALLOWED_ORIGINS`. |
| `POST /v1/webhooks/dodo` | Dodo | Verifies the signature; a paid order issues and emails a key, a full refund revokes it. |
| `POST /v1/admin/licenses` `{email, name?}` | `license:create` | Gives someone a free license. Needs `ADMIN_TOKEN`. |
| `GET /health` | | `{ok: true}` |

Errors are `{error: {code, message}}`, and the message can be shown to the user as is. A 5xx means the API failed. The app treats it as offline and never holds it against the user.

## License keys

Keys are dotsafe's own, so changing payment providers never touches one: only the webhook route would change.

- A key is `DOTSAFE-` and four groups of five characters from a 32-letter alphabet without look-alikes: 100 random bits (`src/keys.ts`).
- D1 stores only its SHA-256 hash and last four characters (for support: "the key ending in 7KQ2"). A leaked database exposes no working key. The email is the only copy, so a lost key means issuing a new license.
- Each license allows 2 devices (`DEVICE_LIMIT` in `src/issue.ts`). Activation takes a slot in one statement, so two at once can't both take the last one.
- A paid order (`payment.succeeded` for `DODO_PRODUCT_ID`) issues one license per payment and emails the key once (`src/issue.ts`). Claiming the email and setting the key happen together; if sending fails, Dodo's retry sends a fresh key, since nobody received the first.
- A full refund revokes the license; a partial one doesn't. Disputes are recorded.

### Giving a license away

```bash
DOTSAFE_ADMIN_TOKEN=… bun run license:create someone@example.com "Their Name"
```

It's issued and emailed like a purchase, with `source = manual`, and the key is printed once. `DOTSAFE_ADMIN_TOKEN` is the Worker's `ADMIN_TOKEN` secret; without that secret the admin routes are off (404). The token is compared in constant time, and the route shares the license rate limit.

## Abuse limits

The code is public, so anyone can script these routes. So (`src/limits.ts`, bindings in `wrangler.jsonc`):

- License and admin routes allow 30 requests a minute per client IP, and checkout 10 (each checkout spends the Dodo API key), per Cloudflare location. Over that, the answer is `429 rate_limited` with `Retry-After: 60`. The app counts a 429 as offline, so a busy shared IP never makes a good license look invalid.
- Request bodies are capped at 16 KB (512 KB for webhooks), with `413 too_large`.
- Every license lookup is by the key's hash. A wrong, made-up or revoked key never changes a record, and one key can't touch another key's devices. Purchases come only from signed webhooks. The database has no public address; only the Worker can reach it.
- Webhooks aren't rate limited: they're signed by Dodo, checked for freshness and handled once each, and limiting them could drop a real purchase.

## What's stored

D1 (schema in `migrations/`) holds:

- **purchases**: payment id, customer id, email, name, amount (smallest currency unit) and currency, paid date, and any refund or dispute.
- **licenses**: id, key hash and last four characters, email and name, `purchase` or `manual`, the payment, device limit, when it was emailed, and when and why it was revoked.
- **activations**: device instance id, its license, the device name, when it was activated, last checked in and deactivated.
- **webhook_events**: ids of handled webhooks, so Dodo's retries apply once.

License keys, emails and names are never logged.

To look something up: `bunx wrangler d1 execute dotsafe-api --env production --remote --command "SELECT id, key_hint, email, revoked_at FROM licenses WHERE email = '…'"`.

## License email

`src/email.ts`, sent through Cloudflare Email Service from `EMAIL_FROM`. Set `EMAIL_REPLY_TO` to an inbox you read, and the email invites replies.

## Setup

```bash
bun install
cp .dev.vars.example .dev.vars   # fill in test-mode keys and your product id
bun run db:migrate:local         # create the local D1 tables
bun run dev                      # http://localhost:8787 (debug builds of the app use this)
bun run test
bun run typecheck
```

In Dodo, create a one-time product with **no** license key entitlement (dotsafe issues the keys), and a webhook pointing at `/v1/webhooks/dodo` with `payment.succeeded`, `refund.succeeded` and `dispute.opened`.

Locally, wrangler doesn't send email; it logs what it would send. Add `--var ADMIN_TOKEN:…` to `bun run dev` to try `license:create` against it with `DOTSAFE_API_URL=http://localhost:8787`.

## Deploy

Cloudflare Workers Builds deploys the `production` env (Worker `dotsafe-api`, live mode) on every push to `main`: root directory `apps/api`, build command `bun install --frozen-lockfile`, deploy command `bunx wrangler d1 migrations apply DB --env production --remote && bunx wrangler deploy --env production`, so schema changes in `migrations/` apply before the code that needs them. The top-level config is the test-mode Worker `dotsafe-api-test`, used by `wrangler dev`.

Vars that aren't secret live in `wrangler.jsonc`; set `DODO_PRODUCT_ID` there for production. Once, before the first deploy with the database:

```bash
bunx wrangler d1 create dotsafe-api                  # put the id in env.production.d1_databases
bunx wrangler email sending enable dotsafe.dev       # adds the SPF and DKIM records
```

Secrets are set once:

```bash
bunx wrangler secret put DODO_API_KEY --env production
bunx wrangler secret put DODO_WEBHOOK_KEY --env production
bunx wrangler secret put ADMIN_TOKEN --env production    # a long random string, e.g. from `openssl rand -hex 32`
```

The production env claims `api.dotsafe.dev` as a custom domain, so the first deploy creates its DNS record. Release builds of the app call `https://api.dotsafe.dev` unless built with `DOTSAFE_API_URL`.
