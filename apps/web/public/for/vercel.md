# Sync and rotate Vercel environment variables

> Keep the source of truth for your Vercel environment variables in dotsafe, and push each new value to Vercel when you rotate it.

Vercel stores environment variables per project and per environment (production, preview, development). Copying a new key into the dashboard by hand, for every project that uses it, is how a rotated key gets missed. dotsafe keeps each secret in one place and pushes the current value to your Vercel projects whenever it changes.

## Connect Vercel

Create an access token at vercel.com/account/tokens, scoped to your team, then:

```bash
dotsafe connect vercel --name web                 # asks for the token
```

For a team, add its ID: `--set team_id=team_…`. The token is stored encrypted in your vault and only used from your Mac.

## Push a secret to a project

```bash
dotsafe secret target add STRIPE_SECRET_KEY --connection web --set project=my-app
dotsafe secret push STRIPE_SECRET_KEY             # push the current value now
```

By default it's set for all three Vercel environments. To limit it:

```bash
dotsafe secret target add STRIPE_SECRET_KEY --connection web --set project=my-app --set environments=production
```

## Rotate once, everywhere

```bash
dotsafe secret rotate STRIPE_SECRET_KEY
```

The new value goes live in every dotsafe environment that links the secret and is pushed to every Vercel project you added. With a provider connected (OpenAI, Cloudflare, AWS), dotsafe also creates the new key for you. Redeploy on Vercel for running deployments to pick it up.

## Local development

Run `next dev` with the same variables, instead of `vercel env pull` into a `.env` file:

```bash
dotsafe run -- next dev
```

See also [dotsafe for Next.js](/for/nextjs) and [Integrations](/docs/integrations).
