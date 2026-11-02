# dotsafe API

A small Cloudflare Worker (Hono) in front of [Dodo Payments](https://dodopayments.com). It handles license activation for the desktop app and CLI, checkout for the website, and Dodo webhooks. It never sees vaults or secrets.

| Route | Used by | What it does |
| --- | --- | --- |
| `POST /v1/licenses/activate` `{license_key, device_name}` | app | Activates a key for a device and checks that it's a dotsafe key. Returns `instance_id`. |
| `POST /v1/licenses/validate` `{license_key, instance_id}` | app | `{valid}` |
| `POST /v1/licenses/deactivate` `{license_key, instance_id}` | app | Frees the device slot. Already-freed slots count as success. |
| `POST /v1/checkout` `{email?}` | website | `{checkout_url}` for the dotsafe product. CORS limited to `ALLOWED_ORIGINS`. |
| `POST /v1/webhooks/dodo` | Dodo | Verifies the Standard Webhooks signature and logs purchase events. |
| `GET /health` | | `{ok: true}` |

Errors are `{error: {code, message}}`, and the message can be shown to the user as is. A 5xx means Dodo or the API failed. The app treats it as offline and never holds it against the user.

Dodo's activation endpoint accepts any merchant's keys, so `activate` also looks the key up with the secret API key and rejects keys whose product isn't `DODO_PRODUCT_ID`. That's why activation goes through this API instead of straight from the app to Dodo.

## Setup

```bash
bun install
cp .dev.vars.example .dev.vars   # fill in test-mode keys and your product id
bun run dev                      # http://localhost:8787 (debug builds of the app use this)
bun run test
bun run typecheck
```

In Dodo, create a one-time product with license keys turned on (set the activation limit you want per purchase), and a webhook pointing at `/v1/webhooks/dodo`.

## Deploy

Cloudflare Workers Builds deploys the `production` env (Worker `dotsafe-api`, live mode) on every push to `main`: root directory `apps/api`, build command `bun install --frozen-lockfile`, deploy command `bunx wrangler deploy --env production`. The top-level config is the test-mode Worker `dotsafe-api-test`, used by `wrangler dev`.

Vars that aren't secret live in `wrangler.jsonc`; set `DODO_PRODUCT_ID` there for production. Secrets are set once:

```bash
bunx wrangler secret put DODO_API_KEY --env production
bunx wrangler secret put DODO_WEBHOOK_KEY --env production
```

The production env claims `api.dotsafe.dev` as a custom domain, so the first deploy creates its DNS record. Release builds of the app call `https://api.dotsafe.dev` unless built with `DOTSAFE_API_URL`.
