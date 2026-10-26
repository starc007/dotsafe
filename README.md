# dotsafe

Local-first, zero-knowledge secret manager for developers. Secrets are encrypted on your machine and synced through storage you already own (iCloud Drive, Google Drive, and more).

## Layout

```
crates/core   # vault format, encryption, sync engine (shared)
crates/cli    # `dotsafe` command-line tool
src-tauri     # desktop app (Tauri 2)
src           # desktop UI (React + Vite)
```

## Development

```bash
bun install
bun run tauri dev          # desktop app
cargo run -p dotsafe-cli   # CLI
```
