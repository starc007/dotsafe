# Rotate your Cloudflare API token

> How to roll a Cloudflare API token without breaking your deploys, what to do when one leaks, and how dotsafe rolls it and updates your Workers in one step.

Rolling a Cloudflare API token gives it a new value with the same permissions, and the previous value stops working. That makes Cloudflare different from most providers: there's no overlap, so everything that uses the token has to get the new value straight away.

## At a glance

| | |
| --- | --- |
| Token formats | User tokens start with `cfut_`, account tokens with `cfat_`. Tokens made before the new format are 40 characters and keep working |
| Environment variable | `CLOUDFLARE_API_TOKEN` (read by Wrangler) |
| Where to roll | My Profile → API Tokens for user tokens; Manage Account → Account API tokens for account tokens |
| Old value after a roll | Invalidated |
| Roll through the API | `PUT /user/tokens/{token_id}/value`, or `PUT /accounts/{account_id}/tokens/{token_id}/value` for account tokens |
| Leak detection | GitHub secret scanning: a token found in a public repo is revoked and you get an email |

The Global API Key is the legacy way in: one per user, with full access and no IP or expiry limits. Use scoped API tokens instead.

## Roll a token by hand

1. Open My Profile → API Tokens in the dashboard (or the account's API tokens for an account-owned token).
2. Open the token's menu, choose **Roll** and confirm.
3. Copy the new value. Cloudflare shows it only once.
4. Update `CLOUDFLARE_API_TOKEN` everywhere it's used: your shell, CI, deploy settings and any Worker that calls the API.
5. Check the new value: `curl https://api.cloudflare.com/client/v4/user/tokens/verify -H "Authorization: Bearer $CLOUDFLARE_API_TOKEN"` should report the token as `active`.

The old value stops working when you roll, so have step 4 ready before step 2. If you can't update everything at once, create a second token with the same permissions first, move each user over, then delete the old one.

When you make the new token, give it only the permissions and zones it needs, and set an expiry and client IP filtering where you can.

## If the token leaked

- **Roll it now.** Cloudflare's guidance for a lost or compromised token is to roll it or replace it.
- **Check what it could do.** Its permissions list the accounts and zones it could change. Review those for changes you didn't make.
- **If it was in a public GitHub repo**, Cloudflare may already have revoked it through GitHub secret scanning and emailed you. You still need a new token everywhere it was used.

## Rotate with dotsafe

dotsafe rolls the token through Cloudflare's API, saves the new value to your vault first, makes it live in every environment that uses it, and pushes it to your deploy targets, such as a Worker secret or a Vercel project.

Connect once with a token that can edit API tokens, then tell dotsafe which token the secret holds:

```bash
dotsafe secret share CLOUDFLARE_API_TOKEN --all   # if it's still a plain variable
dotsafe connect cloudflare_token          # asks for a token with "API Tokens: Edit"
dotsafe secret provider CLOUDFLARE_API_TOKEN --connection "Cloudflare API token" --set token_id=<id>
dotsafe connect cloudflare_workers --set account_id=<id>   # optional: a Worker that holds the token
dotsafe secret target add CLOUDFLARE_API_TOKEN --connection "Cloudflare Workers" --set script=my-worker
```

Then each rotation is one command:

```bash
dotsafe secret rotate CLOUDFLARE_API_TOKEN
```

For an account-owned token, connect with `--set account_id=<id>`. Because Cloudflare invalidates the old value on a roll, there's nothing to retire afterwards. Set a reminder with `dotsafe secret policy CLOUDFLARE_API_TOKEN --every 90`. See [Integrations](/docs/integrations) and [Secrets and rotation](/docs/secrets).

## Sources

Checked on November 11, 2026.

- [API token formats](https://developers.cloudflare.com/fundamentals/api/get-started/token-formats/) (prefixes, verify endpoint, GitHub secret scanning)
- [Roll a token](https://developers.cloudflare.com/fundamentals/api/how-to/roll-token/)
- [Roll a user token's value](https://developers.cloudflare.com/api/resources/user/subresources/tokens/subresources/value/methods/update/) and [an account token's value](https://developers.cloudflare.com/api/resources/accounts/subresources/tokens/subresources/value/methods/update/)
- [Create a token](https://developers.cloudflare.com/fundamentals/api/get-started/create-token/) and [account-owned tokens](https://developers.cloudflare.com/fundamentals/api/get-started/account-owned-tokens/)
- [API keys](https://developers.cloudflare.com/fundamentals/api/get-started/keys/) (Global API Key)
- [Wrangler environment variables](https://developers.cloudflare.com/workers/wrangler/system-environment-variables/)
