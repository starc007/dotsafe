# Integrations

> Rotate keys through OpenAI, Cloudflare and AWS, and push them to Vercel and Workers.

A [secret](/docs/secrets) can have a **provider**, the service that issues its keys, and any number of **targets**, the places its value is deployed. With both set, one rotation asks the provider for a new key, makes it live in every linked environment and pushes it to your deploys.

dotsafe calls these services from your Mac, with admin tokens you give it. The tokens are stored in the vault, encrypted like everything else. Set it all up in Settings → Integrations, or with the CLI below.

## Supported services

| Service | Name | What dotsafe does |
| --- | --- | --- |
| OpenAI | `openai` | Creates and revokes keys for a project |
| Cloudflare API tokens | `cloudflare_token` | Rolls a token's value |
| AWS IAM | `aws_iam` | Creates and deletes a user's access keys |
| Anthropic | `anthropic` | Checks a key you made in the console, then switches the old one off |
| Stripe | `stripe` | Checks a key you made in the dashboard; you revoke the old one there |
| Vercel | `vercel` | Target: sets the variable on a project |
| Cloudflare Workers | `cloudflare_workers` | Target: sets a Worker secret |

## Connect

```bash
dotsafe connect openai --set project_id=proj_…   # asks for the admin token
dotsafe connect vercel --name web
dotsafe connections                              # what's connected
dotsafe reconnect web                            # give a connection a new token
dotsafe disconnect web
```

> [!NOTE]
> An admin token goes into the app once, when you connect, and never comes back out. Give it only the access dotsafe needs.

## Set a provider and targets

```bash
dotsafe secret provider OPENAI_API_KEY --connection OpenAI
dotsafe secret target add OPENAI_API_KEY --connection web --set project=my-app
dotsafe secret push OPENAI_API_KEY        # push the current value to every target now
```

## Rotate

```bash
dotsafe secret rotate OPENAI_API_KEY      # new key from OpenAI, live everywhere, pushed to Vercel
dotsafe secret retire OPENAI_API_KEY      # revoke the old key at OpenAI
```

Each rotation happens in an order that never leaves you without a working key:

1. The new key is saved to the vault first.
2. dotsafe checks that it works.
3. It goes live in every linked environment and is pushed to each target.
4. Where the provider allows it, the old key keeps working until you retire it, or is revoked at once with `--leaked`.

### Anthropic and Stripe

Their keys can only be made in their own console. `dotsafe secret rotate` shows the steps, asks for the new key (or reads it with `--stdin`), checks it, makes it live and pushes it.
