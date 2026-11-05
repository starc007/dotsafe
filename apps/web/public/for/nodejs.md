# Manage Node.js environment variables without dotenv

> Replace dotenv and .env files in Node.js with an encrypted vault: dotsafe injects process.env when your app starts.

Most Node.js apps read secrets from `process.env`, loaded from a `.env` file by `dotenv` or Node's own `--env-file`. dotsafe keeps those variables encrypted on your Mac instead, and sets them when your process starts. Your code doesn't change.

## Switch in three commands

```bash
dotsafe import                                 # ./.env into the development environment
rm .env
dotsafe run -- node server.js                  # or npm run dev, bun dev, pnpm dev
```

You can then drop `dotenv` from your dependencies, or leave it: it doesn't override variables that are already set.

## Different environments

```bash
dotsafe set -e production DATABASE_URL         # asks for the value, so it stays out of shell history
dotsafe run -e production -- node dist/server.js
```

Values can reference each other, so a connection string stays in step with its parts:

```bash
dotsafe set 'DATABASE_URL=postgres://${{ DB_USER }}@${{ DB_HOST }}/app'
```

## Tools that need a file

Some tools only read a `.env` file. Write one that only you can read, and that dotsafe refuses to create unless git ignores it:

```bash
dotsafe export -o .env
```

## Keys for OpenAI and Anthropic

`dotsafe run --proxy -- node server.js` gives the OpenAI and Anthropic SDKs a placeholder key and a local address. dotsafe adds the real key on the way out, so it's never in your process.

Start with the [quick start](/docs/quick-start).
