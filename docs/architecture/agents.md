# Agent access

**Status:** M6 is implemented on macOS and Linux: `dotsafe mcp`, the broker with approvals and grants, the refusal list, the runner, redaction, the secret proxy (also `dotsafe run --proxy`), `generate_secret`, the audit log and custom proxy profiles. Approvals ask for Touch ID (or the login password) on macOS. `dotsafe daemon` runs the same broker without the app. Still proposed: `propose_rotation`, standing approvals for unattended machines, and Windows named pipes.

**`dotsafe daemon`:** the broker (`dotsafe_agent::broker`, shared with the app through a small `Host` trait: vault, grants, how to ask) served from a terminal. It unlocks once (session cache, `DOTSAFE_PASSWORD`, or a prompt) and serves the same socket, refusing to start if the app already does. Approvals are asked in its terminal (once, 15 minutes, an hour, or until it stops); with no terminal on stdin, every request is refused, because nobody is there to approve. `--for 8h` stops it after a while.

**Custom proxy profiles** add keys beyond OpenAI and Anthropic: the key's variable, its SDK's base URL variable, the one `https://` upstream, and the header. They're vault records (`proxy_profile`), not config or repo files, because a profile decides where a key is sent: agents can edit files but not the vault. The proxy checks each one before use (`Profile::custom`: plain https host with no credentials, query or fragment; valid variable names; a plain header that isn't `host`, `cookie` and the like; no built-in variable taken over) and skips any that fail. Manage them in Settings → Agents or with `dotsafe proxy`.

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
| `list_variables(project, env)` | Variable names, and which are shared secrets. **Never values** | No |
| `run_with_secrets(command, cwd, variables, timeout)` | Exit code, redacted stdout/stderr | Yes, unless a grant covers it |
| `run_with_secrets(…, proxy: true)` | Known API keys go through the secret proxy automatically (on by default) | Part of the run's approval |
| `generate_secret(name, generator, env)` | The new secret's name. **The value is generated in the broker and never returned** | Yes |
| `propose_rotation(secret)` (not built yet) | A proposal the user reviews in the app | The user acts on it |

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

- **The approval dialog** is native, from the desktop app. It shows the program, full argv, working directory, which secrets, and who asked. Duration choices: once, 15 minutes, 1 hour, until lock.
- **Touch ID confirms** on macOS (LocalAuthentication, `DeviceOwnerAuthentication`, so the login password works on Macs without Touch ID). It's enforced in Rust inside `answer_agent_request`, not in the webview, so approving needs someone at the Mac even if the webview were compromised. A cancelled prompt leaves the request open to try again or deny. It's on by default; turning it off in Settings → Agents needs Touch ID too. Linux approves with the click.
- **If the vault is locked,** unlocking and approving happen in one prompt.
- **Refused for agents, whatever the user clicks:**
  - Shells and interpreters given inline code: `sh`, `bash -c`, `zsh -c`, `node -e/-p/--eval`, `python -c`, `ruby -e`, `perl -e`, `deno eval`, `bun -e`.
  - Commands that dump the environment: `env`, `printenv`, `set`, `export -p`.
  - `dotsafe` itself: `get` and `export` print values, and a vault left open by `dotsafe unlock` would let them run without a password.
  - These are a speed bump, not a boundary. See [Limits](#limits).
- **Scripts the agent can edit:** when the command runs a file (`node check.js`, or a program inside the working directory), the dialog lists it with when it last changed, and warns when that was in the last 10 minutes, since the agent may have just written it. A grant covers each file's contents as approved (SHA-256, or size and change time above 10 MB): edit the file and the next run asks again. Files reached indirectly (`bun test`, `npm run dev`, `--require x.js`) aren't detected.
- **Headless policies (proposed):** for a daemon with nobody at its terminal, standing grants created by the user and stored in the vault (not the repo, which the agent can edit). Until then, an unattended daemon refuses everything.

## Runner

- **The broker spawns the process, not the agent.** The environment is a fixed allowlist of the broker's own variables (`HOME`, `USER`, `SHELL`, `LANG`, `TERM`, `TMPDIR`, `TZ`, …), the agent's `PATH` (to find programs the way the agent would), and only the granted variables. Nothing else from the agent passes through, so `DYLD_*`, `LD_*` or `NODE_OPTIONS` can't be slipped in. Stdin is closed.
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
- Values shorter than 6 characters aren't redacted: "true" or "3000" would mangle normal output, and they're rarely secrets. They're still injected.
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

- **Profiles** say which base-URL variable to set, the upstream host, and how to inject the key. Built in: OpenAI (`OPENAI_API_KEY` → `OPENAI_BASE_URL`, `Authorization: Bearer`) and Anthropic (`ANTHROPIC_API_KEY` → `ANTHROPIC_BASE_URL`, `x-api-key`). A base URL already in the environment (an Azure or gateway endpoint, HTTPS only) becomes the route's upstream. Custom profiles (more providers, other header formats) are still to do.
- **In the broker,** a run's routes exist only while the command runs. The approval dialog marks which variables will be proxied, and the result tells the agent. Agents can pass `proxy: false` for code that can't take a custom base URL.
- **Redirects aren't followed** (the key never goes anywhere but the route's upstream), and the proxy asks for unencoded bodies so it can stream them through as they arrive.
- **It's not an open proxy.** Each route has one fixed upstream host. HTTPS upstream only, with certificate verification.
- **The 128-bit random token** in the path stops other local users from using a route over loopback. Each grant gets its own token, which stops working when the grant expires.
- **Limits:** works only where the SDK or tool accepts a base-URL override (most AI and API SDKs do). No TLS interception and no system proxy changes. Requests and errors are counted per grant and shown in the app.
- **Humans benefit too:** `dotsafe run --proxy -- bun dev` keeps API keys out of dev servers, crash reporters and logs.
- **Later:** a local TCP proxy for Postgres and MySQL that handles authentication, so database passwords get the same treatment.

## Audit log

- Record kind `audit` (record version 3): one record per device per day (id = UUIDv5(device, day)), so only one device ever writes each. Events:

```json
{ "at": 1793038221399, "actor": "Claude Code", "action": "run", "outcome": "allowed",
  "summary": "bun test", "environment": "my-api/development",
  "variables": ["DATABASE_URL", "OPENAI_API_KEY"], "detail": "exit 0" }
```

- `action` is `run` or `generate`. `outcome` is `allowed`, `denied`, `refused` or `failed`. Names only, never values.
- Encrypted and synced like every other record. Events merge by id. After 90 days a day's record is emptied and tombstoned (a tombstone wins over older copies).
- **Settings → Agents → Activity** in the desktop app shows the newest events from every device, and `dotsafe audit` prints them. Filtering by agent or secret, and revoking a grant from an event, are still to do.
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

1. ✅ `dotsafe mcp` with `list_projects` and `list_variables` (names only). No format change.
2. ✅ Broker, IPC, approvals, runner, refusal list (macOS and Linux; Windows and Touch ID still to do).
3. ✅ Redaction.
4. ✅ Secret proxy with built-in profiles, plus `dotsafe run --proxy` for humans.
5. ✅ `generate_secret` and the audit log.
6. ✅ Touch ID on approval (macOS).
7. Windows named pipes, standing approvals. (✅ custom proxy profiles, ✅ `dotsafe daemon`.)
