# Contributing

Thanks for helping with dotsafe. This covers how to set up the repo and send a change. [AGENTS.md](AGENTS.md) has the rules for changing the code safely, and it's worth reading before a first PR.

## Layout

```
crates/core              vault format, encryption, sync engine (shared)
crates/stores            S3/R2 and WebDAV storage, keychain-backed store opener
crates/agent             agent access: broker, MCP, redaction, secret proxy
crates/integrations      provider key rotation and deploy-target pushes
crates/cli               the dotsafe command-line tool
apps/desktop/src-tauri   desktop app (Tauri 2)
apps/desktop/src         desktop UI (React, Vite, Tailwind, shadcn/ui)
apps/api                 licensing and checkout API (Cloudflare Worker)
apps/web                 site and docs (Astro)
```

## Setup

It's a bun workspace with one lockfile at the root.

```bash
bun install                # once, at the root
bun run tauri dev          # desktop app
bun run dev                # UI only, in a browser, with a mock backend
cargo run -p dotsafe-cli   # CLI
bun run web                # site and docs at http://localhost:3000
```

`bun run dev` serves the UI at http://localhost:1420 with an in-memory mock (`apps/desktop/src/dev/mockBackend.ts`). Unlock with `password123`, or add `?setup` to start at setup. The mock only loads in dev outside Tauri and never ships.

Set `DOTSAFE_CONFIG_DIR` to point the app or CLI at a throwaway config, so testing never touches your real vault. Debug builds use the license API at `http://localhost:8787`; set `DOTSAFE_API_URL` at build time to use another.

## Sending a change

1. Work on a branch and open a PR into `main`.
2. Run `scripts/check.sh` and make sure it passes. It builds the frontend, runs clippy and every test. There's no CI right now, so this is the only check.
3. Use [Conventional Commits](https://www.conventionalcommits.org): `feat(cli): …`, `fix(core): …`, `docs: …`.
4. Sign off your commits (`git commit -s`) to confirm you wrote the change and can share it under the MIT license ([Developer Certificate of Origin](https://developercertificate.org)).

Changes to the vault format, crypto or sync need tests and a version bump. See [docs/vault-format.md](docs/vault-format.md) first.

## More

- [docs/architecture](docs/architecture/README.md): storage, device keys, rotation, agents and the threat model
- [docs/design.md](docs/design.md): the brand and the website
- [docs/releasing.md](docs/releasing.md): how releases are made
- [apps/api/README.md](apps/api/README.md): the licensing API
