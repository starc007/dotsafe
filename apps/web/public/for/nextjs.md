# Manage Next.js environment variables without .env files

> Keep your Next.js secrets encrypted with dotsafe and inject them into next dev and next build, instead of .env.local.

Next.js reads `.env`, `.env.local` and their per-environment variants. That works, until a `.env.local` is shared over Slack, committed by mistake, or read by a coding agent. dotsafe keeps the same variables encrypted on your Mac and hands them to Next.js when it starts.

## Move your .env.local into dotsafe

```bash
dotsafe import .env.local                       # into the development environment
dotsafe import .env.production -e production    # if you have one
rm .env.local .env.production
```

## Run Next.js with your variables

```bash
dotsafe run -- next dev
dotsafe run -e production -- next build
```

Next.js uses variables already in the environment before anything from a `.env` file, so nothing else changes: `process.env.DATABASE_URL` works on the server as before.

## Mind NEXT_PUBLIC_

Anything prefixed `NEXT_PUBLIC_` is copied into the browser bundle when you run `next build`, so it's public by design. Keep real secrets (`STRIPE_SECRET_KEY`, `DATABASE_URL`) without the prefix, and remember a `NEXT_PUBLIC_` value is fixed at build time: rebuild after you change it.

```bash
dotsafe list                                   # check which names carry the prefix
dotsafe scan                                   # find any secret value in your source
```

## Staging and production

Make staging inherit production and override only what differs:

```bash
dotsafe inherit production -e staging
dotsafe set -e staging NEXT_PUBLIC_API_URL=https://staging.example.com
```

## Deploying to Vercel

dotsafe can push a secret to your Vercel project every time it's rotated. See [dotsafe for Vercel](/for/vercel).

Start with the [quick start](/docs/quick-start).
