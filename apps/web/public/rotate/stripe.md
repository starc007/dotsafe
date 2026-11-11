# Rotate your Stripe secret key

> How to rotate a Stripe secret or restricted key with an overlap, what to do when one leaks, and how dotsafe checks the new key and pushes it to every deploy.

Stripe can rotate a key for you in the Dashboard: it makes the replacement at once and lets the old key keep working for up to 7 days while you move over. There's no API for it, so the rotation itself always starts in the Dashboard.

## At a glance

| | |
| --- | --- |
| Key formats | Secret `sk_live_` and `sk_test_`, restricted `rk_live_` and `rk_test_`, publishable `pk_` (safe to expose) |
| Environment variable | Stripe's docs use `STRIPE_API_KEY`; many apps call it `STRIPE_SECRET_KEY` |
| Where to rotate | Dashboard → Developers → API keys → the key's menu → **Rotate key** |
| Old key after rotating | Keeps working until the expiry you choose: now, or a later time up to 7 days away |
| Rotate through the API | No |
| Leak detection | If Stripe finds an exposed key, it tells you to rotate it, and sometimes deactivates it |

A new live key is shown only once, so copy it straight away.

Stripe recommends restricted keys over secret keys for new work: give each part of your app only the permissions it needs, and limit a key to your servers' IP addresses where you can.

## Rotate a key by hand

1. On the API keys page, open the key's menu and choose **Rotate key**.
2. Pick an expiry for the old key that leaves time to move over (up to 7 days). For longer, create a second key by hand instead.
3. Copy the new key and put it in your secret store or environment, never in code.
4. Redeploy everything that uses it.
5. Watch the old key's request logs. Once its traffic has stayed at zero, let it expire or delete it.

Rotate on a schedule, and whenever someone with access leaves.

## If the key leaked

Stripe's steps for a compromised key:

1. **Rotate it now**, with an expiry just long enough to deploy the new key.
2. **Update your backend** and check the new key's request logs.
3. **Delete the compromised key.** Deleting is immediate; its logs stay.
4. **Look for damage**: connected accounts, payouts and bank accounts you don't recognize. Reverse payouts and refund fraudulent charges.
5. **Rotate your other secret and restricted keys** too, and rotate again if you find another exposure.

Stripe never asks you for your secret key.

## Rotate with dotsafe

Stripe keys can only be made in the Dashboard, so dotsafe guides the rotation: you rotate the key in Stripe, paste the new one, and dotsafe does everything after that. No admin token is needed.

```bash
dotsafe secret share STRIPE_SECRET_KEY --all        # if it's still a plain variable
dotsafe connect stripe
dotsafe secret provider STRIPE_SECRET_KEY --connection Stripe
dotsafe connect vercel --name web                   # optional: push to a Vercel project
dotsafe secret target add STRIPE_SECRET_KEY --connection web --set project=my-app
```

Then:

```bash
dotsafe secret rotate STRIPE_SECRET_KEY    # shows the steps, then asks for the new key
```

dotsafe saves the new key to your vault, checks it with Stripe, makes it live in every environment that uses it (in every project linked to the secret), and pushes it to your deploy targets. Stripe has no API for revoking keys, so dotsafe links you to the Dashboard to delete the old one there. Once you have, mark it retired with `dotsafe secret retire STRIPE_SECRET_KEY --by-hand`. That's also true for `--leaked`: delete the leaked key in the Dashboard straight away.

See [Integrations](/docs/integrations) and [Secrets and rotation](/docs/secrets).

## Sources

Checked on November 11, 2026.

- [API keys](https://docs.stripe.com/keys) (formats, rotating, expiry, restricted keys)
- [Best practices for managing secret API keys](https://docs.stripe.com/keys-best-practices) (exposed keys, `STRIPE_API_KEY`)
- [Protecting against compromised API keys](https://support.stripe.com/questions/protecting-against-compromised-api-keys)
