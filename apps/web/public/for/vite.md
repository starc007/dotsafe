# Manage Vite environment variables without .env files

> Inject Vite's VITE_ variables and your server secrets from an encrypted dotsafe vault, instead of .env files.

Vite loads `.env` files and exposes only variables prefixed `VITE_` to your client code, through `import.meta.env`. dotsafe replaces the files: your variables live encrypted on your Mac and are passed to Vite when it starts.

## Move your .env files into dotsafe

```bash
dotsafe import                                 # ./.env into the development environment
dotsafe import .env.production -e production
rm .env .env.production
```

## Run Vite with your variables

```bash
dotsafe run -- vite
dotsafe run -e production -- vite build
```

Variables already set in the environment take priority over `.env` files in Vite, and `VITE_` ones still reach `import.meta.env` as before.

## Only VITE_ reaches the browser

Every `VITE_` value is written into your built JavaScript, where anyone can read it. Keep secrets such as API keys for your own backend without the prefix. In `vite.config.ts`, all variables are available through `process.env`, which is the place for build-time secrets.

```bash
dotsafe scan                                   # find vault values in your source or build output
dotsafe scan --install-hook                    # and check every commit
```

## A backend next to it

If your Vite app has an API server, give both the same environment:

```bash
dotsafe run -- npm run dev                     # e.g. concurrently runs vite and the server
```

Start with the [quick start](/docs/quick-start).
