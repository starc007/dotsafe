# Manage Cloudflare Workers secrets

> Push secrets to your Cloudflare Workers from dotsafe, and rotate them in one step instead of running wrangler secret put by hand.

Workers keep secrets per Worker, usually set with `wrangler secret put` and kept locally in `.dev.vars`. dotsafe keeps each secret encrypted in one place, injects it into local development, and pushes the current value to every Worker that uses it when it changes.

## Connect your Cloudflare account

Create an API token with the **Workers Scripts: Edit** permission, then:

```bash
dotsafe connect cloudflare_workers --name workers --set account_id=<your account id>
```

The token is stored encrypted in your vault and only used from your Mac.

## Push a secret to a Worker

```bash
dotsafe secret target add OPENAI_API_KEY --connection workers --set script=my-worker
dotsafe secret push OPENAI_API_KEY               # set it on the Worker now
```

Add as many Workers as use the secret. Each is updated on every rotation.

## Rotate in one step

```bash
dotsafe secret rotate OPENAI_API_KEY
```

With OpenAI connected as the secret's provider, dotsafe creates the new key, makes it live in every linked environment, and sets it on each Worker. Retire the old key when you're ready: `dotsafe secret retire OPENAI_API_KEY`.

## Local development

`wrangler dev` reads local secrets from `.dev.vars`. Write it from dotsafe instead of by hand: the file is readable only by you, and dotsafe refuses to write it unless git ignores it.

```bash
dotsafe export -o .dev.vars
wrangler dev
```

## Cloudflare API tokens

dotsafe can also rotate Cloudflare API tokens themselves: connect `cloudflare_token` as a provider. See [Integrations](/docs/integrations).
