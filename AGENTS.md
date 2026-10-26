# AGENTS.md

Guidance for AI coding agents working in this repo. `README.md` covers the product and user-facing commands; this file covers how to change the code safely.

## What this is

dotsafe is a local-first, zero-knowledge secret manager for developers. Secrets are encrypted on the device and synced through storage the user already owns (iCloud Drive, Google Drive, Dropbox, any synced folder). The only dotsafe server is the licensing API in `api/`, and it never sees vaults or secrets. It ships as a Tauri 2 desktop app and a `dotsafe` CLI that share one Rust core.

## Layout

```
crates/core   Vault format, crypto, merge/sync, config, storage (store.rs). Shared by app and CLI.
crates/agent  Agent access: broker protocol, local socket, refusal policy, redaction, runner. `dotsafe mcp` (CLI) and the broker (desktop) use it.
crates/stores Network storage backends (S3/R2) and `open(config)`, which picks the right store and reads keys from the keychain.
crates/cli    `dotsafe` binary (clap). End-to-end tests in crates/cli/tests.
src-tauri     Desktop backend: commands.rs (UI-callable commands), secrets.rs (shared secret commands), state.rs (session, auto-lock, clipboard), licensing.rs (license API client), sync.rs (background sync for bucket vaults), broker.rs (agent broker and approvals).
src           Desktop UI: React 19 + Vite + Tailwind v4 + shadcn/ui + motion.
api           Hono Worker on Cloudflare for licenses, checkout and Dodo webhooks. See api/README.md.
docs/vault-format.md   The on-disk format and its known limitations. Read before touching crates/core.
docs/architecture/     Proposed direction: storage backends, device keys, rotation, agent access, threat model.
```

## Commands

```bash
bun install
bun run tauri dev                                   # desktop app
bun run dev                                         # UI only, in a browser, with a mock backend
bun run build                                       # tsc + vite build
cargo run -p dotsafe-cli -- <args>                  # CLI
cargo test --workspace                              # all Rust tests
cargo clippy --workspace --all-targets -- -D warnings
cd api && bun run test && bun run typecheck         # API
scripts/check.sh                                    # everything below, in one go
```

**Run `scripts/check.sh` before every push and merge, and make sure it passes.** GitHub Actions is switched off for this repo (out of quota), so nothing else will catch a failure. The script does what CI did (`.github/workflows/ci.yml`, kept for when it's turned back on):

- Builds the frontend first, because the desktop crate embeds `dist/` at compile time.
- Runs clippy with `-D warnings`, then the Rust tests.
- Typechecks and tests `api/`.

`scripts/check.sh rust` or `scripts/check.sh api` runs half.

- **Use the current stable Rust.** Newer clippy releases add lints, so an old toolchain can pass locally and fail CI. Run `rustup update stable` when in doubt.
- **It only covers your own OS.** CI also built on Linux and Windows, so keep `cfg(unix)` code and paths obviously correct for the others. The agent crate can be clippy-checked for Windows with `cargo clippy -p dotsafe-agent --target x86_64-pc-windows-msvc`.

## Ground rules

### Git

- Conventional Commits (`feat(cli): …`, `fix(core): …`, `refactor(desktop): …`, `ci: …`, `docs: …`).
- No `Co-Authored-By` or other AI attribution trailers in commits.
- Work on a branch and open a PR into `main`; don't commit to `main` directly.

### Security (this is a secrets app; treat these as hard rules)

- **Crypto stays in `crates/core/src/crypto.rs`.** Use the existing primitives (XChaCha20-Poly1305, Argon2id, HKDF-SHA256, HMAC-SHA256 from RustCrypto). Never add new algorithms, hand-rolled crypto, or `rand`-based nonces.
- **Format changes need a version bump.** Any change to `vault.json`, `head.json` or record files needs a `HEADER_VERSION`/`RECORD_VERSION`/`HEAD_VERSION` bump, an upgrade path from the previous version, and a matching update to `docs/vault-format.md`.
- **Vaults go through `Store`.** `crates/core/src/vault.rs` never touches the filesystem directly; backends implement `Store` (`store.rs`). Writes use conditional puts and merge-and-retry.
- **Network code stays out of core.** `crates/core` has no HTTP client; S3 lives in `crates/stores`. `CachedStore` (core) wraps any remote and must never hold its index lock across network calls.
- **Storage keys live in the OS keychain** (`dotsafe_stores::save_credentials`), never in `config.json`, the vault, or the webview.
- **Keep AAD bindings.** Wrapped keys and records are bound to the vault id and their own id via AAD. Keep it that way.
- **Decrypted values only reach the webview through `reveal_variable` and `reveal_secret`.** Listing commands return names and metadata only. Copying happens in Rust (`copy_variable`, `copy_secret`), never via the JS clipboard API.
- **Linked variables resolve through `Secrets`.** `Environment::entry` returns a value or a secret link; resolve with `vault.secrets()?.value(entry)` or `.resolve(&env)`. `Environment::value` is the variable's own value only, so don't use it where a linked secret should count.
- **Webview permissions stay minimal.** `src-tauri/capabilities/default.json` grants core, window dragging and the folder picker only. Don't grant clipboard, filesystem or shell access to the webview. Keep the strict CSP in `tauri.conf.json`.
- **Wipe key material.** Wrap passwords and key material in `Zeroizing`. `SecretKey` is zeroized on drop; don't copy its bytes into plain `Vec`s or `String`s.
- **No test backdoors.** Never add a bypass to the unlock flow (env vars, debug passwords, skip flags) for testing.

### Rust

- Match the existing style: small modules, doc comments that explain why, `thiserror` errors in core, `anyhow` in the CLI.
- **Core tests:** live in `crates/core/tests/` (`vault.rs`, `format_v2.rs`) and use cheap Argon2 params (`FAST`). Add a test for every format, crypto or merge change, including the tamper cases. `tests/fixtures/v1-vault` is a real v1 vault (password `fixture-password`) for upgrade tests; never regenerate it with current code.
- **CLI tests:** run the real binary with `DOTSAFE_CONFIG_DIR`, `DOTSAFE_NO_KEYCHAIN=1` and `DOTSAFE_PASSWORD`, so they never touch the user's real config or keychain.
- **Don't run `cargo remove` blindly.** It silently deletes the `[profile.dev.package.blake2]` override in the root `Cargo.toml`. That override keeps Argon2 fast in debug builds; without it, tests get about 10× slower. Re-add it if it disappears.

### Frontend

- **Use shadcn/ui components.** Install them with `bunx --bun shadcn@latest add <name>`; they land in `src/components/ui/`. Reuse what's there before adding more. Don't hand-roll primitives (buttons, inputs, dialogs, menus), and don't put several components in one file. App-specific components go in `src/components/`, one per file.
- **beUI components** (the user's library, https://beui.dev) take precedence where one exists. Tabs use beUI's motion tabs in `src/components/motion/tabs.tsx`. Get beUI source through the beUI MCP server (`get_component`) or `bunx --bun shadcn add @beui/<name>`.
- **Colours come from the shadcn tokens in `src/index.css`.** No hardcoded colours. `--shell` is the outer gray of cards and modals, and `--destructive-solid` is for solid delete buttons.
- **Theme:** the user picks System, Light or Dark in the sidebar's appearance menu. `src/lib/theme.ts` stores the choice, toggles `.dark` on `<html>`, and sets the native window theme. Read it with `useTheme()`, and never let theming throw during startup.
- **Cards and modals use `src/components/Surface.tsx`:** a gray shell containing a header (icon tile, title, description, optional action) and a raised inner card holding the content, with footer actions right-aligned. Surfaces are borderless: the shell, inner card and header tiles are separated by fill only, with no rings, borders or shadows (dialogs keep their drop shadow). Don't build ad-hoc bordered panels. Buttons, inputs and tabs are pill-shaped.
- **Modals use `SurfaceDialog`** (forms, imports) or `ConfirmDialog` (destructive confirmations). No inline editing in lists.
- **Imports keep paths in Rust.** Files come from `pick_env_files` or a window drop, and the paths stay in Rust. Previews return names and changes only. Keep it that way.
- **Animations use `motion`** (`import { motion } from "motion/react"`) with the shared timings in `src/lib/motion.ts`. Keep them short and eased-out, with no bouncy springs on content. The app is wrapped in `MotionConfig reducedMotion="user"`. Any imperative animation (`useAnimate`) must also check `useReducedMotion()`.
- **Keyboard first.** Every action needs a keyboard path. Shortcuts live only in `SHORTCUTS` (`src/lib/shortcuts.ts`): register handlers with `useShortcut`, show hints with `ShortcutKeys`, and set `aria-keyshortcuts`. The Settings page lists them automatically. Shortcuts stand aside while a dialog or menu is open.
- **Call the backend through the typed wrappers in `src/lib/api.ts`.** Errors arrive as `ApiError` with a `kind`. A `locked` error sends the UI back to the unlock screen automatically.
- **Write copy in short, active voice:** "No variables yet", "Wrong password." Every list needs loading (Skeleton), empty and error states.

## Testing the desktop app

- **Use a throwaway config.** `DOTSAFE_CONFIG_DIR=/some/tmp/dir bun run tauri dev` keeps testing away from the real config. The default vault location is the user's real iCloud Drive, so never create test vaults there.
- **Browser mode:** `bun run dev` serves the UI at http://localhost:1420 with an in-memory mock (`src/dev/mockBackend.ts`). Unlock with `password123`; add `?setup` to start at setup, `?bucket` to start with a vault in a bucket, and `?offline` to make license server calls fail. In bucket setup, the bucket name `existing` has a vault (password `password123`) and `offline` can't be reached. `STRIPE_SECRET_KEY` in my-api is seeded as an overdue shared secret linked into both environments. `?agent` sends a fake agent approval request after unlocking. License keys containing `LIMIT` or `REVOKED` act out those answers. The mock is dev-only and stripped from production builds. Keep it in step when command signatures change.
- **Password fields can't be automated in the native app.** macOS secure input blocks synthetic keystrokes into password fields, so unlock flows can't be driven by automation there. Use the browser mock for UI flows and say clearly what wasn't verified natively.
- **The app runs in WebKit (WKWebView), not Chromium.** The browser preview can hide WebKit-only bugs. Example: a container that fades its opacity while some children animate on their own layers can leave the other children painted after the fade. Give the fading container `will-change-[opacity]`, as the row actions in `VariableList.tsx` do.
- **Agents never get values.** No broker request or MCP tool returns a secret; values only reach processes the broker starts after the user approves. Keep the refusal list in `crates/agent/src/policy.rs` and the environment allowlist in `run.rs` strict.
- **Licensing and sync never block the vault.** License requests run off the session lock (`off_thread` in commands.rs), and bucket sync uses the shared `Arc<Vault>` outside it (`sync.rs`). Network failures are `offline` errors, never revocations or data loss. Keep it that way.
- **Known gap:** the desktop commands (`src-tauri/src/commands.rs`) have no automated tests yet.
