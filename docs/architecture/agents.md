# Agent access

**Status:** proposed. Milestone M6. It can start before format v2; only the audit log needs v2.

## The problem

Coding agents (Claude Code, Cursor, Codex, and so on) need secrets to run tests, start dev servers and call APIs. Today they get them by reading `.env` files, so every secret in a project sits in the agent's context, its logs, and wherever a prompt injection sends it.

## The hard truth

**Any process that holds a secret can leak it.** If an agent can run `dotsafe run -- <anything>`, it can run `node -e "console.log(process.env.KEY)"`, base64-encode the value past any filter, or send it to a URL. So "agents can't read secrets" is only true if agents never control a process holding plaintext. The design comes in layers, from weakest to strongest guarantee:

| Layer | Guarantee |
|---|---|
| 1. References, not values | dotsafe never gives an agent plaintext |
| 2. Approval broker | Secrets only reach commands the user approved, for a limited time |
| 3. Output redaction | Accidental leaks in command output are scrubbed before the agent sees them |
| 4. Secret proxy | For HTTP API keys, the agent's processes never hold the key at all |
| 5. Audit log | Every grant and use is recorded |

## Components

```
 agent host (Claude Code…)                      desktop app / dotsafe daemon
 ┌──────────────────────┐   stdio    ┌────────────┐  unix socket / named pipe   ┌───────────────────────────┐
 │ agent ─ MCP client ──┼──────────▶ │ dotsafe mcp│ ──────────────────────────▶ │ broker                    │
 └──────────────────────┘            │ (no keys)  │   owner-only, peer-checked  │  policy · approvals (UI)  │
                                     └────────────┘                             │  runner ─▶ child process  │
                                                                                │  redactor                 │
   child process ── http://127.0.0.1:PORT/p/<token>/ ──────────────────────────▶│  secret proxy ─▶ upstream │
                                                                                │  audit log                │
                                                                                └───────────────────────────┘
```

- **`dotsafe mcp`:** a stdio MCP server the agent host launches. It holds no keys, can't unlock anything, and only forwards requests to the broker.
- **Broker:** runs inside the desktop app, or `dotsafe daemon` on machines without it. It's the only place that decrypts on behalf of an agent.
- **IPC:**
  - **macOS/Linux:** a Unix socket in the app data directory, mode `0600`, and peer credentials checked (`getpeereid` / `SO_PEERCRED`) to be the same user.
  - **Windows:** a named pipe with an ACL for the current user only.
- **The client's name** (from the MCP `clientInfo`) is shown to the user, but never trusted for decisions.

## MCP tools

| Tool | Returns | Needs approval |
|---|---|---|
| `list_projects` | Project and environment names | No |
| `list_secrets(project, env)` | Names, age, source, where used. **Never values** | No |
| `run(argv, cwd, secrets, timeout)` | Exit code, redacted stdout/stderr | Yes, unless a grant covers it |
| `proxy(secret, profile)` | Env vars to set (`OPENAI_BASE_URL=…`, a dummy key) | Yes |
| `generate_secret(name, generator, env)` | The new secret's name. **The value is generated in the broker and never returned** | Yes |
| `propose_rotation(secret)` | A proposal the user reviews in the app | The user acts on it |

There's no `reveal`, `get` or `export` tool, and there never will be.

`generate_secret` matters: an agent scaffolding a project can create `JWT_SECRET` or `SESSION_KEY` without ever seeing it.

## Grants and approvals

A grant is an exact permission:

```
Grant {
  id, client_label,
  program: resolved absolute path, argv: exact list (or exact prefix, if the user chose it),
  cwd, secrets: [refs], env_names,
  expires_at, max_uses,
}
```

- **The approval dialog** is native, from the desktop app. It shows the program, full argv, working directory, which secrets, and who asked. Duration choices: once, 15 minutes, 1 hour, until lock. Touch ID confirms.
- **If the vault is locked,** unlocking and approving happen in one prompt.
- **Refused for agents, whatever the user clicks:**
  - Shells and interpreters given inline code: `sh`, `bash -c`, `zsh -c`, `node -e/-p/--eval`, `python -c`, `ruby -e`, `perl -e`, `deno eval`, `bun -e`.
  - Commands that dump the environment: `env`, `printenv`, `set`, `export -p`.
  - These are a speed bump, not a boundary. See [Limits](#limits).
- **Repo files the agent can edit:** when the command runs a script in the workspace (`bun test`, `npm run dev`), the dialog warns that the agent can change that script. It shows uncommitted changes in the files the command will run, using `git status`.
- **Headless policies:** for `dotsafe daemon` without a UI, the user creates standing grants in the app. They're stored in the vault, not in the repo, because the agent can edit the repo.

## Runner

- **The broker spawns the process, not the agent.** The environment is a minimal copy of the parent's, plus only the granted secrets. Stdin is closed by default.
- **Output** streams back through the redactor to the MCP client.
- **When the grant expires,** in-flight processes are left alone and new runs need approval.

## Redaction

- For every injected value of 6 or more characters, the redactor matches:
  - the raw value;
  - base64 (standard and URL-safe, at all three byte offsets);
  - lower- and upper-case hex;
  - URL encoding;
  - JSON string escaping.
- **Streaming:** Aho-Corasick over the stream, keeping an overlap buffer as long as the longest pattern, so values split across chunks are still caught.
- **Replacement:** `[dotsafe:STRIPE_SECRET_KEY]`.
- Values shorter than 6 characters can't be redacted reliably. They're refused for agent runs, with an explanation.
- **This catches accidents** (a stack trace printing a connection string), not adversaries.

## Secret proxy

The strongest layer: the agent's process gets a URL, not a key.

```
child process                               broker proxy                                   upstream
OPENAI_BASE_URL=http://127.0.0.1:7341/p/9f2c…/v1
OPENAI_API_KEY=dotsafe-proxied   ──HTTP──▶  look up token 9f2c… → grant
                                            strip incoming auth headers
                                            add  Authorization: Bearer <real key>  ──HTTPS──▶ api.openai.com
                                            stream the response back (SSE included)
```

- **Profiles** for common providers say which base-URL variable to set, the upstream host, and how to inject the key: OpenAI (`OPENAI_BASE_URL`), Anthropic (`ANTHROPIC_BASE_URL`), Stripe, GitHub, Resend, and more. Users can add a custom profile (upstream host, header name, format).
- **It's not an open proxy.** Each route has one fixed upstream host. HTTPS upstream only, with certificate verification.
- **The 128-bit random token** in the path stops other local users from using a route over loopback. Each grant gets its own token, which stops working when the grant expires.
- **Limits:** works only where the SDK or tool accepts a base-URL override (most AI and API SDKs do). No TLS interception and no system proxy changes. Requests and errors are counted per grant and shown in the app.
- **Humans benefit too:** `dotsafe run --proxy -- bun dev` keeps API keys out of dev servers, crash reporters and logs.
- **Later:** a local TCP proxy for Postgres and MySQL that handles authentication, so database passwords get the same treatment.

## Audit log

- New record kind `audit` (format v2): one record per device per day, holding an append-only list of events:

```json
{ "stamp": {…}, "actor": { "kind": "agent", "label": "Claude Code" }, "action": "run",
  "grant": "uuid", "program": "/opt/homebrew/bin/bun", "argv_summary": "bun test",
  "secrets": ["uuid", "uuid"], "outcome": { "exit": 0, "redactions": 1 } }
```

- Encrypted and synced like every other record. Merges as a union of events. Kept for 90 days by default.
- **Activity** in the desktop app shows it: filter by agent, secret or grant, and revoke a grant from an event.
- **Honest limit:** any device with the vault key could forge or drop events. The log shows what happened through dotsafe; it isn't evidence against a compromised device.

## Setup

```bash
dotsafe mcp install claude-code    # writes the MCP server entry into the agent's config
dotsafe mcp install cursor
```

The setup screen also offers a snippet for the project's `AGENTS.md`: "Secrets are managed by dotsafe. Use the dotsafe MCP tools; never read or create `.env` files."

## Limits

- **An approved command holds its secrets.** If the agent can change what that command does (by editing a test, or a `package.json` script), it can make the command leak them, through the network if not through output. Short grants, the warning about edited files, and the audit log reduce this. Only the proxy removes it.
- **Users can approve something dangerous.** The dialog must be clear and calm, and default to "once".
- **Malware on the device is out of scope** ([threat-model.md](threat-model.md)).

## Milestones

1. `dotsafe mcp` with `list_projects` and `list_secrets` (names only). No format change.
2. Broker, IPC, approvals, runner, refusal list.
3. Redaction.
4. Secret proxy with built-in profiles, plus `dotsafe run --proxy` for humans.
5. `generate_secret` and the audit log (format v2).
