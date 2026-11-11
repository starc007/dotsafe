# Rotate your OpenAI API key

> How to rotate an OpenAI API key without downtime, what to do when one leaks, and how dotsafe issues a new key and makes it live everywhere in one step.

OpenAI keys don't replace each other: a new key works alongside the old one until you revoke the old one. That makes rotation safe if you do it in order: create, switch, check, then revoke.

## At a glance

| | |
| --- | --- |
| Environment variable | `OPENAI_API_KEY`, read automatically by OpenAI's SDKs |
| Kinds of key | User-owned project keys, service account keys (for deployed apps and CI), and Admin API keys (for managing the organization) |
| Where to manage them | [platform.openai.com/settings/organization/api-keys](https://platform.openai.com/settings/organization/api-keys) |
| Old key after a new one is made | Keeps working until you revoke it |
| Rotate through the API | An Admin API key can create service accounts with their own keys, and delete them |
| Expiry | Keys can have an expiry, and admins can set a maximum key lifetime for the organization |

For anything deployed, use a service account key rather than your own user key. The service account keeps working when people leave, and its keys can be swapped without touching anyone's account.

## Rotate a key by hand

OpenAI's own process:

1. Create the replacement key in the project, before the old one expires.
2. Put it everywhere `OPENAI_API_KEY` is used: your shell, CI, and each deploy.
3. Check the new key works, for example by watching requests succeed on the [usage page](https://platform.openai.com/usage).
4. Revoke the old key.

Set an expiry when you create a key, so a forgotten one can't live forever, and keep keys out of browser code, mobile apps, source control and logs.

## If the key leaked

- **Revoke it now**, then create a replacement and roll it out. Every minute it stays valid, someone else can spend your credit.
- **Check usage** on the usage page for requests you didn't make.
- **Find where it leaked**: a commit, a log, a client bundle, a chat with an AI agent. Remove it there too, or the new key will leak the same way.

## Rotate with dotsafe

dotsafe connects to your OpenAI organization with an Admin API key. Each rotation creates a new service account with a fresh key in your project, saves the key to your vault first, checks it, makes it live in every environment that uses it and pushes it to your deploy targets. The old key keeps working until you retire it.

```bash
dotsafe secret share OPENAI_API_KEY --all           # if it's still a plain variable
dotsafe connect openai --set project_id=proj_…      # asks for the admin key
dotsafe secret provider OPENAI_API_KEY --connection OpenAI
dotsafe connect vercel --name web                   # optional: push to a Vercel project
dotsafe secret target add OPENAI_API_KEY --connection web --set project=my-app
```

Then rotate, and retire the old key once your deploys have it:

```bash
dotsafe secret rotate OPENAI_API_KEY
dotsafe secret retire OPENAI_API_KEY     # deletes the old key at OpenAI
```

dotsafe can delete only the keys it made. After the first rotation, delete the key you started with on the API keys page, then run `dotsafe secret retire OPENAI_API_KEY --by-hand`. From then on, `retire` does it for you, and `dotsafe secret rotate OPENAI_API_KEY --leaked` deletes the old key straight away. `dotsafe secret policy OPENAI_API_KEY --every 90` reminds you when it's due.

### Keep the key away from AI agents

When Claude Code, Cursor or Codex run a command through dotsafe, OpenAI calls go through a local proxy that adds the real key, so the command never holds it. See [A secret manager for AI agents](/docs/agents).

See [Integrations](/docs/integrations) and [Secrets and rotation](/docs/secrets).

## Sources

Checked on November 11, 2026.

- [API keys](https://developers.openai.com/api/docs/guides/api-keys) (kinds of key, service accounts, expiry)
- [Production best practices: API keys](https://developers.openai.com/api/docs/guides/production-best-practices#api-keys) (rotation steps, key governance, safety)
- [Project API keys in the Admin API](https://developers.openai.com/api/reference/resources/organization/subresources/projects/subresources/api_keys)
- [Libraries](https://platform.openai.com/docs/libraries) (`OPENAI_API_KEY`)
