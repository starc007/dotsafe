# dotsafe

Local-first, zero-knowledge secret manager for developers. Secrets are encrypted on your machine and synced through storage you already own (iCloud Drive, Google Drive, and more).

## Layout

```
crates/core   # vault format, encryption, sync engine (shared)
crates/cli    # `dotsafe` command-line tool
src-tauri     # desktop app (Tauri 2)
src           # desktop UI (React + Vite)
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
dotsafe unlock --for 8h           # cache the key in the OS keychain; `dotsafe lock` to forget it
```

The first write in a directory creates a project and a `.dotsafe.json` link file. It only holds ids, so commit it; other machines connected to the same vault then pick the project up. `dotsafe link <name>` attaches a directory to an existing project.

For CI and scripts, `DOTSAFE_PASSWORD` supplies the master password without a prompt. Anything that can read the environment can read it, so prefer `dotsafe unlock` on your own machine.

## Development

```bash
bun install
bun run tauri dev          # desktop app
cargo run -p dotsafe-cli   # CLI
```
