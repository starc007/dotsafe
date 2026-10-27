# dotsafe

Local-first, zero-knowledge secret manager for developers. Secrets are encrypted on your machine and synced through storage you already own (iCloud Drive, Google Drive, and more).

## Layout

```
crates/core   # vault format, encryption, sync engine (shared)
crates/stores # S3/R2 storage backend and the keychain-backed store opener
crates/cli    # `dotsafe` command-line tool
src-tauri     # desktop app (Tauri 2)
src           # desktop UI (React + Vite)
api           # licensing and checkout API (Cloudflare Worker), see api/README.md
```

## CLI

```bash
dotsafe init                      # create a vault (iCloud Drive by default) or connect to one
dotsafe import                    # import ./.env into this project's development env
dotsafe set API_KEY               # prompts for the value, so it stays out of shell history
dotsafe set -e production A=1 B=2
dotsafe get API_KEY
dotsafe list
dotsafe unset API_KEY
dotsafe run -- bun dev            # runs the command with the variables injected
dotsafe export -o .env            # for tools that need a file: owner-only, refused unless gitignored
dotsafe scan                      # find vault values in this checkout (raw or encoded); names only
dotsafe scan --install-hook       # pre-commit hook that runs `dotsafe scan --staged`
dotsafe unlock --for 8h           # cache the key in the OS keychain; `dotsafe lock` to forget it
dotsafe init --bucket my-vaults --endpoint https://<account>.r2.cloudflarestorage.com
dotsafe sync                      # vaults in a bucket: exchange changes now
dotsafe secret share STRIPE_KEY --all   # one shared secret, linked wherever it had the same value
dotsafe secret rotate STRIPE_KEY        # new value everywhere it's linked; old one kept as previous
dotsafe secret rotate STRIPE_KEY --leaked --generate
dotsafe secret retire STRIPE_KEY        # once the old value is revoked with its provider
dotsafe secret history STRIPE_KEY       # versions and notes, never values
dotsafe secret due                      # what's due under each secret's rotation policy
```

**AI agents.** `dotsafe mcp` lets Claude Code, Cursor and other MCP agents run commands with your secrets without ever seeing them. Agents can list names. When one asks to run a command, the dotsafe app shows exactly what will run and with which variables, and you allow it once or for a while, confirming with Touch ID. The output comes back with secret values replaced by `[dotsafe:NAME]`. OpenAI and Anthropic keys go through a local proxy, so the command never even holds them, and agents can add random secrets (`JWT_SECRET`…) without seeing the value. Everything is recorded in an encrypted audit log (`dotsafe audit`). Set it up with `claude mcp add dotsafe -- dotsafe mcp`, or `dotsafe mcp setup` for other agents.

`dotsafe run --proxy -- bun dev` gives your own dev servers the same protection: the process sees a placeholder key and a local URL, never the real key.

**In the app.** Press ⌘K anywhere to find a variable in any project and copy it, or jump to an environment. The value goes straight to the clipboard and is cleared after 30 seconds. If dotsafe reads a record that's older than one it saw before, or one sealed with a key the vault no longer has, the sidebar shows "Vault needs a look" and Settings → Security explains what happened.

**Devices.** Turn on Touch ID unlock in Settings → Devices (the password is still asked every two weeks). Add a Mac with a one-time code instead of typing the password there. If a device is lost, remove it: dotsafe switches the vault to a new key and re-encrypts everything, so it can't read anything new.

**Shared secrets.** A variable can link to a shared secret instead of holding its own value. Rotate the secret once and every linked environment, in any project, gets the new value. Each secret keeps its history, so a bad rotation can be rolled back or an earlier value restored.

**Vaults in a bucket.** Instead of a synced folder, a vault can live in any S3-compatible bucket (Cloudflare R2, AWS S3, Backblaze B2, MinIO). The bucket only ever holds encrypted files. Each machine keeps an encrypted copy, so dotsafe works offline and syncs when it's back online. The access key is kept in the OS keychain, or read from `DOTSAFE_S3_ACCESS_KEY_ID` and `DOTSAFE_S3_SECRET_ACCESS_KEY` for CI. Give that key access to this one bucket only.

The first write in a directory creates a project and a `.dotsafe.json` link file. It only holds ids, so commit it; other machines connected to the same vault then pick the project up. `dotsafe link <name>` attaches a directory to an existing project.

For CI and scripts, `DOTSAFE_PASSWORD` supplies the master password without a prompt. Anything that can read the environment can read it, so prefer `dotsafe unlock` on your own machine.

## Development

```bash
bun install
bun run tauri dev          # desktop app
cargo run -p dotsafe-cli   # CLI
scripts/check.sh           # build, clippy and all tests; run before pushing
```

`bun run dev` on its own serves the UI at http://localhost:1420 with an in-memory mock backend (`src/dev/mockBackend.ts`, unlock with `password123`, add `?setup` to start at setup). It is only loaded in dev outside Tauri and never ships.

Set `DOTSAFE_CONFIG_DIR` to point the app or CLI at a throwaway config, so testing never touches your real vault.

### Desktop security model

- Decrypted values reach the webview only when you reveal or edit one. Lists carry names only.
- Copying happens in Rust. The clipboard is cleared after 30 seconds unless you copied something else since.
- The vault locks after 15 minutes without activity.
- The webview has no clipboard or filesystem permissions, only the folder picker. The CSP blocks remote content.

### Licensing

Settings → License activates a key through the dotsafe API (`api/`), which talks to Dodo Payments. Only the key and an activation id are stored, in the machine's config, never in the vault. The app rechecks about twice a day. Being offline never locks anything: after 30 days without a check the license just shows as unverified. Debug builds use the API at `http://localhost:8787`; set `DOTSAFE_API_URL` at build time to use another.
