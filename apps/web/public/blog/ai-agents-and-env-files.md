# Your AI coding agent shouldn't read your .env

> Coding agents need real API keys to run tests and dev servers. Letting them read .env files puts every key in their context. Here's how dotsafe lets agents use keys without seeing them, and where that stops working.

Claude Code, Cursor and Codex are most useful when they can run your project: the test suite, a migration, the dev server. All of that needs keys. The easy way to give them keys is the `.env` file already sitting in the repo.

That works. It also means every key in the file is now in the agent's context. From there it can end up in a transcript, a log, a summary it writes, a commit, or wherever a prompt injection tells it to send things. You never pasted a key into a chat, but the agent read it anyway.

## Telling the agent not to look doesn't fix it

The first fix people reach for is a rule: don't open `.env`. That stops the agent reading the file. It doesn't stop it running `node -e "console.log(process.env)"`, or a test that prints its config when it fails, or a dev server that logs the database URL on boot. If a process the agent starts can see the key, the agent can see the key.

So the real rule is harder:

**Any process that holds a secret can leak it.**

Filters can't fix that. A command can base64 the value, split it, or post it to a URL. "Agents can't read secrets" is only true if agents never control a process that holds the plaintext.

dotsafe doesn't pretend to solve this with one trick. It stacks layers, from weakest to strongest:

| Layer | What it guarantees |
| --- | --- |
| Names, not values | dotsafe never gives an agent a value |
| Approve each command | Secrets only reach commands you approved, for as long as you said |
| Redact output | Accidental leaks in output are scrubbed before the agent reads it |
| Proxy the key | For API keys, the process never holds the key at all |
| Audit log | Every request is recorded |

## Layer 1: names, not values

The agent talks to dotsafe over MCP, with four tools:

- `list_projects`: projects and their environments.
- `list_variables`: variable names, and which are linked to shared secrets. Never values.
- `run_with_secrets`: ask to run a command with an environment's variables.
- `generate_secret`: create a random value, like `JWT_SECRET`, without seeing it.

There's no "get value" tool. The agent knows `STRIPE_SECRET_KEY` exists and can ask to use it. That's all.

## Layer 2: you approve the exact command

When the agent calls `run_with_secrets`, the app shows the exact command, the working directory, the environment and which variables it gets. You allow it once, for 15 minutes, for an hour, or until the vault locks, and confirm with Touch ID. That check runs in Rust, not in the webview, so approving needs someone at the Mac.

An approval is narrow on purpose. It covers:

- the resolved program path, so a different `bun` earlier on the `PATH` doesn't match
- the exact arguments
- the working directory
- the environment and the variable names
- the scripts it runs, by fingerprint, so editing `check.js` after you approved it asks again

Approvals live in memory and end when the vault locks.

Commands run directly, never through a shell, and some are refused whatever you click:

- programs that print the environment: `env`, `printenv`, `set`, `export`
- shells given a command string (`bash -c`), and interpreters given inline code (`node -e`, `python -c`, `ruby -e` and the rest)
- wrappers that hide the real program: `xargs`, `sudo`, `nohup`, `timeout`, `eval`
- `dotsafe` itself, since `get` and `export` print values

The child process gets a small fixed environment plus the secrets: no `LD_PRELOAD`, `DYLD_*` or `NODE_OPTIONS` passed through from anywhere, and stdin closed.

None of this is a wall. It's a speed bump that stops the obvious one-liners and keeps the approval prompt honest about what actually runs.

## Layer 3: scrub the output

The agent gets the command's output back, with every secret replaced by `[dotsafe:NAME]`.

Matching the raw value isn't enough, because values get re-encoded on the way out. dotsafe also catches hex, base64 (standard and URL-safe, at any byte alignment), URL encoding and JSON string escaping. It all runs as one Aho-Corasick search over the output. Values shorter than 6 characters are skipped, since redacting every `true` and `3000` would wreck normal output.

This catches accidents: the stack trace that prints a connection string, the debug log that dumps config. It won't stop a command written to get around it.

## Layer 4: the process never holds the key

This is the only layer that removes the risk instead of reducing it.

For OpenAI and Anthropic keys, the command doesn't get the key. It gets `OPENAI_API_KEY=dotsafe-proxied` and a base URL pointing at a proxy dotsafe runs on `127.0.0.1`. The SDKs already read both from the environment, so the code doesn't change. The proxy:

- strips whatever auth the command sends and adds the real key
- forwards to the provider's one HTTPS address, never anywhere else, and never follows redirects
- streams the response back, server-sent events included
- serves each run under its own random 128-bit path, removed when the run ends, so other local users can't borrow it

A prompt-injected agent has nothing to leak. It can still make API calls while the run lasts, which is why runs are short and approved.

You can add other APIs as proxy profiles. Profiles live in the encrypted vault, never in a file in the repo, so an agent can't add one that points your key at its own server.

## Layer 5: write it all down

Every request goes into an encrypted audit log in the vault, names only, from every device. `dotsafe audit` shows what each agent asked for, which variables it got, and when.

## Where this stops

An approved command still holds its secrets, unless they're behind the proxy. If the agent can change what that command does, by editing a test or a `package.json` script, it can make the command leak them, through the network if not through output. dotsafe fingerprints the scripts a command runs directly, but files reached indirectly (`bun test` loading a test file, `npm run dev` running a script) aren't covered.

So the practical advice is short:

- Approve once by default, and keep longer grants short.
- Read the command before you approve it, especially right after the agent edited something.
- Put API keys behind the proxy wherever the SDK takes a base URL.

## Try it

Install the CLI from the app (**Settings → Agents → Install**), then add dotsafe to your agent:

```bash
claude mcp add dotsafe -- dotsafe mcp
```

The setup for Cursor, Codex and other MCP apps is in [A secret manager for AI agents](/docs/agents), with step-by-step guides for [Claude Code](/for/claude-code), [Cursor](/for/cursor) and [Codex](/for/codex). The design notes, limits included, are in the [source on GitHub](https://github.com/starc007/dotsafe).
