# Secret manager for Claude Code

> Let Claude Code run your tests, migrations and dev servers with real API keys, without ever handing it a value.

Claude Code works best when it can run your project: the test suite, a migration, the dev server. Those need your API keys. Pasting keys into the chat or leaving them in a `.env` file the agent can read puts them in its context, its logs and anything it writes.

dotsafe is a secret manager built for this. Claude Code asks dotsafe to run a command, you approve it with Touch ID, and the command gets your secrets as environment variables. Claude Code gets the output back with every secret replaced by `[dotsafe:NAME]`.

## Set up

Install the `dotsafe` command from the app (**Settings → Agents → Install**), then add it to Claude Code:

```bash
claude mcp add dotsafe -- dotsafe mcp
```

The dotsafe app must be open and unlocked. Without it, run `dotsafe daemon` in a terminal and approve requests there.

## What Claude Code can do

- **List** your projects, environments and variable names. Never values.
- **Run a command** with an environment's variables, such as `bun test` or `bunx prisma migrate dev`, once you approve it.
- **Create a random secret**, such as `JWT_SECRET`, without seeing the value.

Commands that would only print your secrets, like `env` or `printenv`, are refused outright.

## Keep API keys out of the process too

For OpenAI and Anthropic keys, dotsafe goes further. The command Claude Code runs gets a placeholder key and a local address, and dotsafe's proxy adds the real key on the way to `api.openai.com` or `api.anthropic.com`. Even a command that tries to print or send the key has nothing to leak.

## You stay in control

- Every request shows the exact command and which variables it gets. Allow it once, or for a while.
- Every request is written to an encrypted audit log: `dotsafe audit`.
- Your secrets stay encrypted on your Mac. dotsafe has no server that could hand them out.

> [!WARNING]
> A command you approve can use the secrets it's given. Read what you approve, and keep grants short.

Read the full guide: [A secret manager for AI agents](/docs/agents).
