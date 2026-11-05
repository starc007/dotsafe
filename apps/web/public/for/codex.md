# Secret manager for Codex

> Run Codex tasks that need real API keys, with the keys injected by dotsafe and kept out of what Codex reads.

Codex can write and run code across your project, and real projects need credentials. dotsafe lets Codex run a command with your secrets once you approve it, without the values ever reaching the model.

## Set up

Install the `dotsafe` command from the app (**Settings → Agents → Install**), then add it to Codex:

```bash
codex mcp add dotsafe -- /usr/local/bin/dotsafe mcp
```

The Codex CLI, app and IDE extension share this setting. The dotsafe app must be open and unlocked; without it, run `dotsafe daemon` and approve requests in that terminal.

## How a run works

1. Codex lists variable names to see what the project needs.
2. It asks to run a command, such as `pytest` or `npm run build`, with the development environment.
3. dotsafe shows you the command and the variables. You approve it with Touch ID, once or for a while.
4. The command runs with your secrets. Codex gets the output with each value replaced by `[dotsafe:NAME]`.

## Safer by default

- Commands that only print secrets (`env`, `printenv`) are refused.
- OpenAI and Anthropic keys go through a local proxy, so the process never holds them.
- Codex can create new random secrets, like a session key, without seeing them.
- Every request is in an encrypted audit log that syncs with your vault.

> [!WARNING]
> A command you approve can use the secrets it's given. Read what you approve, and keep grants short.

Full details: [A secret manager for AI agents](/docs/agents).
