# Secret manager for Cursor

> Give Cursor's agent your project's API keys to run commands with, without putting a single value in its context.

Cursor's agent can run terminal commands for you, and most real commands need secrets: a database URL, a Stripe key, an OpenAI key. A `.env` file in the workspace is one file read away from the model's context.

With dotsafe, the agent asks dotsafe to run the command instead. You approve it with Touch ID, the command gets your variables, and the agent reads back output with every secret value replaced by `[dotsafe:NAME]`.

## Set up

Install the `dotsafe` command from the app (**Settings → Agents → Install**), then add it to `~/.cursor/mcp.json`:

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

Use the full path: Cursor opened from the Dock doesn't look in `/usr/local/bin`. `dotsafe mcp setup` prints this block with the right path.

## Move your .env out of the workspace

```bash
dotsafe import          # reads ./.env into the project's development environment
rm .env
```

Now the agent can see that `DATABASE_URL` exists, but not what it is. When it needs to run `npm run dev` or a migration, it asks dotsafe, and you approve it.

## What the agent gets

| | Without dotsafe | With dotsafe |
| --- | --- | --- |
| Variable names | In `.env` | Listed by dotsafe |
| Values | Readable in `.env` | Never returned |
| Command output | Raw, keys included | Keys replaced by `[dotsafe:NAME]` |
| OpenAI and Anthropic keys | In the process | Behind a local proxy |
| Record of use | None | Encrypted audit log |

> [!WARNING]
> A command you approve can use the secrets it's given. Read what you approve.

More in [A secret manager for AI agents](/docs/agents).
