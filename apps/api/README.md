# dotsafe API

A small Cloudflare Worker (Hono) in front of [Dodo Payments](https://dodopayments.com). It handles license activation for the desktop app and CLI, checkout for the website, and Dodo webhooks. It keeps a record of purchases, license keys and activated devices in D1, and emails each new license key. It never sees vaults or secrets.

| Route | Used by | What it does |
| --- | --- | --- |
| `POST /v1/licenses/activate` `{license_key, device_name}` | app | Activates a key for a device and checks that it's a dotsafe key. Returns `instance_id`. |
| `POST /v1/licenses/validate` `{license_key, instance_id}` | app | `{valid}` |
| `POST /v1/licenses/deactivate` `{license_key, instance_id}` | app | Frees the device slot. Already-freed slots count as success. |
| `POST /v1/checkout` `{email?}` | website | `{checkout_url}` for the dotsafe product. CORS limited to `ALLOWED_ORIGINS`. |
| `POST /v1/webhooks/dodo` | Dodo | Verifies the Standard Webhooks signature, records the event in D1, and emails new license keys. |
| `GET /health` | | `{ok: true}` |

Errors are `{error: {code, message}}`, and the message can be shown to the user as is. A 5xx means Dodo or the API failed. The app treats it as offline and never holds it against the user.

## What's stored

D1 (schema in `migrations/`) holds:

- **purchases**: payment id, customer id, email, name, amount (smallest currency unit) and currency, paid date, and any refund or dispute.
- **licenses**: key id, the key itself, payment, customer and email, activation limit, expiry, when it was emailed, and when and why it was revoked.
- **activations**: device instance id, its license, the device name, when it was activated, last checked in and deactivated.
- **webhook_events**: ids of handled webhooks, so Dodo's retries apply once.

Webhooks write with upserts, so a retried or out-of-order delivery lands the same way. A failed webhook answers 500 and Dodo retries it. Writes from the license routes are best effort: a database problem never fails an activation or check. License keys, emails and names are never logged.

To look something up: `bunx wrangler d1 execute dotsafe-api --env production --remote --command "SELECT * FROM purchases WHERE email = '…'"`.

## License email

Dodo's own **License Key Created** email is turned off (Dodo: Settings → Communication preferences). When a key is issued (`entitlement_grant.created`), the webhook checks it's a dotsafe key, records it, and sends the email in `src/email.ts` through Cloudflare Email Service, from `EMAIL_FROM`. Each key is claimed before sending, so it goes out once even if Dodo delivers twice; a failed send gives the claim back and Dodo's retry tries again. Set `EMAIL_REPLY_TO` to an inbox you read, and the email invites replies.

Dodo's activation endpoint accepts any merchant's keys, so `activate` also looks the key up with the secret API key and rejects keys whose product isn't `DODO_PRODUCT_ID`. That's why activation goes through this API instead of straight from the app to Dodo.

## Setup

```bash
bun install
cp .dev.vars.example .dev.vars   # fill in test-mode keys and your product id
bun run db:migrate:local         # create the local D1 tables
bun run dev                      # http://localhost:8787 (debug builds of the app use this)
bun run test
bun run typecheck
```

In Dodo, create a License Key entitlement (activation limit 2, no expiration), attach it to a one-time product, turn off the License Key Created customer email, and add a webhook pointing at `/v1/webhooks/dodo` with `payment.succeeded`, `refund.succeeded`, `dispute.opened`, `entitlement_grant.created`, `entitlement_grant.delivered` and `entitlement_grant.revoked`.

Locally, wrangler doesn't send email; it logs what it would send.

## Deploy

Cloudflare Workers Builds deploys the `production` env (Worker `dotsafe-api`, live mode) on every push to `main`: root directory `apps/api`, build command `bun install --frozen-lockfile`, deploy command `bunx wrangler deploy --env production`. The top-level config is the test-mode Worker `dotsafe-api-test`, used by `wrangler dev`.

Vars that aren't secret live in `wrangler.jsonc`; set `DODO_PRODUCT_ID` there for production. Once, before the first deploy with the database:

```bash
bunx wrangler d1 create dotsafe-api                  # put the id in env.production.d1_databases
bunx wrangler email sending enable dotsafe.dev       # adds the SPF and DKIM records
```

Schema changes ship as new files in `migrations/`. Apply them before the deploy that needs them; adding it to the Workers Builds deploy command does it on every push: `bunx wrangler d1 migrations apply DB --env production --remote && bunx wrangler deploy --env production`.

Secrets are set once:

```bash
bunx wrangler secret put DODO_API_KEY --env production
bunx wrangler secret put DODO_WEBHOOK_KEY --env production
```

The production env claims `api.dotsafe.dev` as a custom domain, so the first deploy creates its DNS record. Release builds of the app call `https://api.dotsafe.dev` unless built with `DOTSAFE_API_URL`.
