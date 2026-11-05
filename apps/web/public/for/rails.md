# Manage Rails secrets and ENV without .env files

> Use dotsafe for the ENV side of a Rails app: API keys and database URLs encrypted on your Mac, injected into bin/rails.

Rails has encrypted credentials for secrets that ship with the app, and `ENV` for everything that differs per machine or environment, often loaded from `.env` by `dotenv-rails`. dotsafe takes over the `ENV` side: variables stay encrypted on your Mac and are set when Rails starts.

## Move your .env into dotsafe

```bash
dotsafe import                                 # ./.env into the development environment
rm .env
```

## Run Rails with your variables

```bash
dotsafe run -- bin/rails server
dotsafe run -- bin/dev                          # Procfile.dev: web, jobs and assets together
dotsafe run -- bin/rails test
```

`ENV.fetch("STRIPE_SECRET_KEY")` and `database.yml`'s `<%= ENV["DATABASE_URL"] %>` work as before.

## Keep the master key safe too

`RAILS_MASTER_KEY` unlocks your encrypted credentials. Store it in dotsafe instead of `config/master.key` on every machine:

```bash
dotsafe set -e production RAILS_MASTER_KEY     # asks for the value
dotsafe run -e production -- bin/rails console
```

## Rotate keys in one place

If several apps use the same Stripe or OpenAI key, make it a shared secret and rotate it once:

```bash
dotsafe secret share STRIPE_SECRET_KEY --all
dotsafe secret rotate STRIPE_SECRET_KEY
```

Start with the [quick start](/docs/quick-start).
