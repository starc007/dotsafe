# Rotate your Anthropic API key

> How to rotate a Claude API key without downtime, what to do when one leaks, and how dotsafe checks the new key, makes it live everywhere and switches the old one off.

Anthropic API keys are made only in the Claude Console: the Admin API can read, rename and switch keys off, but it can't create them. So rotation is half by hand, half automatic. You make the new key; everything after that can be scripted.

## At a glance

| | |
| --- | --- |
| Key format | API keys start with `sk-ant-api03-`; Admin API keys with `sk-ant-admin` |
| Environment variable | `ANTHROPIC_API_KEY`, read by the SDKs and CLI |
| Where to manage them | [platform.claude.com/settings/keys](https://platform.claude.com/settings/keys) |
| Create through the API | No. Keys are created in the Console only |
| Switch off through the API | Yes: the Admin API sets a key's status to `inactive` |
| Leak detection | GitHub secret scanning: a key found in a public repo is deactivated and you get an email |

## Rotate a key by hand

Anthropic suggests rotating keys on a schedule, such as every 90 days:

1. In the Console, choose **Create key**, in the same workspace as the old one.
2. Put it everywhere `ANTHROPIC_API_KEY` is used, and deploy.
3. Once nothing uses the old key, deactivate it (or delete it from the key's menu).

Use separate keys for each environment and purpose, so you can switch one off without breaking the rest. Keys can have an expiry; the Admin API lists it as `expires_at`.

## If the key leaked

Anthropic's steps for a compromised key:

- **Revoke it now**: on the API keys page, open the key's menu and choose **Delete API key**, then **Create key** for its replacement.
- **Check the Logs and Usage pages** for requests you didn't make.
- **Limit the damage**: set spend limits in Billing and rate limits for the workspace, and take care with auto-reload.
- **Contact Anthropic support** if you still see activity you don't recognize.

If the key was pushed to a public GitHub repo, Anthropic may already have deactivated it and emailed you. Store the replacement in a secret manager, not in the repo.

## Rotate with dotsafe

dotsafe guides the part you do by hand and automates the rest. Connect once with an Admin API key, which dotsafe uses only to switch old keys off:

```bash
dotsafe secret share ANTHROPIC_API_KEY --all   # if it's still a plain variable
dotsafe connect anthropic                  # asks for the admin key (sk-ant-admin…)
dotsafe secret provider ANTHROPIC_API_KEY --connection Anthropic
```

Then rotate:

```bash
dotsafe secret rotate ANTHROPIC_API_KEY    # shows the steps, then asks for the new key
dotsafe secret retire ANTHROPIC_API_KEY    # switches the old key off at Anthropic
```

`rotate` asks for the key you made in the Console (or reads it with `--stdin`), saves it to your vault, checks it with Anthropic, makes it live in every environment that uses it, and pushes it to your deploy targets. The old key keeps working until you retire it, or is switched off at once with `--leaked`. dotsafe finds each key's ID through the Admin API, so this works from the first rotation, including for the key you started with.

### Keep the key away from AI agents

When Claude Code or another agent runs a command through dotsafe, Anthropic calls go through a local proxy that adds the real key, so the command never holds it. See [Secret manager for Claude Code](/for/claude-code).

See [Integrations](/docs/integrations) and [Secrets and rotation](/docs/secrets).

## Sources

Checked on November 11, 2026.

- [Admin API](https://docs.claude.com/en/api/administration-api) (admin keys, what it can and can't do with keys)
- [Get an API key](https://docs.claude.com/en/api/admin-api/apikeys/get-api-key) (key format, status, workspace)
- [API key best practices](https://support.claude.com/en/articles/9767949-api-key-best-practices-keeping-your-keys-safe-and-secure) (rotation, GitHub secret scanning)
- [What should I do if I suspect my API key has been compromised?](https://support.claude.com/en/articles/8384961-what-should-i-do-if-i-suspect-my-api-key-has-been-compromised)
- [Get started](https://docs.claude.com/en/docs/get-started) (`ANTHROPIC_API_KEY`)
