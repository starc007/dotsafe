# A secret manager for AI agents

> Let Claude Code, Cursor, Codex and other MCP agents run commands with your secrets, without dotsafe ever handing them a value.

Coding agents need your keys to run tests, migrations and dev servers. With dotsafe they run those commands only after you approve, and no request an agent makes ever returns a secret.

## Set up

First install the command-line tool: in the app, **Settings → Agents → Install**. Then add dotsafe to your agent.

**Claude Code** (in a terminal):

```bash
claude mcp add dotsafe -- dotsafe mcp
```

**Codex** (in a terminal; the Codex app and IDE extension share it):

```bash
codex mcp add dotsafe -- /usr/local/bin/dotsafe mcp
```

**Cursor, Claude Desktop, Windsurf and other MCP apps:** add this to their MCP config, for Cursor `~/.cursor/mcp.json`. Use the full path: apps opened from the Dock don't look in `/usr/local/bin`.

```json
{
  "mcpServers": {
    "dotsafe": {
      "command": "/usr/local/bin/dotsafe",
      "args": ["mcp"]
    }
  }
}
```

`dotsafe mcp setup` prints the same, with the path of the `dotsafe` you ran.

Step-by-step guides: [Claude Code](/for/claude-code), [Cursor](/for/cursor) and [Codex](/for/codex).

`dotsafe mcp` holds no keys. It passes requests to the dotsafe app, which must be running and unlocked. Without the app, run `dotsafe daemon` in a terminal and approve requests there, or from any other terminal with `dotsafe approve`. On a server, see [Servers and cron jobs](/docs/servers).

## What an agent can do

- List projects, environments and variable names. Never values.
- Ask to run a command with an environment's variables.
- Add a random secret, such as `JWT_SECRET`, without seeing the value it gets.

## Approving a command

When an agent asks to run something, the app shows the exact command and which variables it gets. Allow it once or for a while, and confirm with Touch ID. Commands that would just print values, like `env` or `printenv`, are refused outright.

The agent gets the output back with every secret value replaced by `[dotsafe:NAME]`, including common encodings of it.

## The secret proxy

Redacting output isn't enough if the command itself could send a key somewhere. For OpenAI and Anthropic keys, dotsafe goes further: the command gets a placeholder key and a local URL, and dotsafe's proxy adds the real key on the way to the one HTTPS address it belongs to. The process never holds it.

The same proxy protects your own dev servers:

```bash
dotsafe run --proxy -- bun dev
```

Add other APIs as proxy profiles, in Settings → Agents or with the CLI:

```bash
dotsafe proxy add Mistral \
  --key-var MISTRAL_API_KEY \
  --base-url-var MISTRAL_BASE_URL \
  --upstream https://api.mistral.ai \
  --base-path /v1
dotsafe proxy list
dotsafe proxy remove Mistral
```

Profiles are stored in the vault, never in a file in your repo, so an agent can't point a key at a server of its own.

## Agents on a cloud machine

An agent on a cloud VM, working while you're away, can use a share instead of your vault. Create one on your Mac with only what the agent needs, then start the daemon on the machine with its token:

```bash
dotsafe share create agent -e development --only OPENAI_API_KEY,DATABASE_URL --for 7d   # on your Mac
DOTSAFE_SHARE=dss_… dotsafe daemon --share --unattended --detach                       # on the machine
```

The agent connects with `dotsafe mcp` as usual. It sees the share's variable names, OpenAI and Anthropic keys go through the proxy, and output is redacted. The machine needs no vault, password or license, and generating secrets is refused, because a share can't be changed there.

- **`--unattended`** runs every command the agent asks for, without asking anyone. Commands that would just print values are still refused. Leave it off to approve in the daemon's terminal, or from another one with `dotsafe approve --share`, which reads the same token.
- **Give the token to the daemon only,** never to the agent's own environment. The commands the daemon runs never get it.
- **Each run fetches the snapshot again,** so a rotated key reaches the agent at once. Revoking the share on your Mac stops the daemon serving it.

## Limits

> [!WARNING]
> A command you approve holds its secrets in its environment, and can do what it likes with them. Redaction catches accidents, like a stack trace that prints a key, not a command written to get around it. Read what you approve, and keep grants short. Keys behind the proxy are the exception: the process never has them.

With a share, the daemon holds every value in it, and so can anyone with root on that machine. With `--unattended`, the share's variables are the only limit, so keep the list short and the share brief. Its requests are logged in the daemon's output (`daemon.log` when detached), not in your vault's audit log.

## Audit log

Every agent request is written to an encrypted audit log in the vault, with names only. It covers every device.

```bash
dotsafe audit --limit 50
```
