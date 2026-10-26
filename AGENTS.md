# AGENTS.md

Guidance for AI coding agents working in this repo. `README.md` covers the product and user-facing commands; this file covers how to change the code safely.

## What this is

dotsafe is a local-first, zero-knowledge secret manager for developers. Secrets are encrypted on the device and synced through storage the user already owns (iCloud Drive, Google Drive, Dropbox, any synced folder). There is no dotsafe server. It ships as a Tauri 2 desktop app and a `dotsafe` CLI that share one Rust core.

## Layout

```
crates/core   Vault format, crypto, merge/sync, config. Shared by app and CLI.
crates/cli    `dotsafe` binary (clap). End-to-end tests in crates/cli/tests.
src-tauri     Desktop backend: commands.rs (UI-callable commands), state.rs (session, auto-lock, clipboard).
src           Desktop UI: React 19 + Vite + Tailwind v4 + shadcn/ui + motion.
docs/vault-format.md   The on-disk format and its known limitations. Read before touching crates/core.
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
```

CI (`.github/workflows/ci.yml`) runs clippy with `-D warnings` and the tests on macOS, Linux and Windows. `bun run build` must run first because the desktop crate embeds `dist/` at compile time. Run the same checks before pushing.

## Ground rules

### Git

- Conventional Commits (`feat(cli): …`, `fix(core): …`, `refactor(desktop): …`, `ci: …`, `docs: …`).
- No `Co-Authored-By` or other AI attribution trailers in commits.
- Work on a branch and open a PR into `main`; don't commit to `main` directly.

### Security (this is a secrets app; treat these as hard rules)

- **Crypto stays in `crates/core/src/crypto.rs`.** Use the existing primitives (XChaCha20-Poly1305, Argon2id, HKDF-SHA256, HMAC-SHA256 from RustCrypto). Never add new algorithms, hand-rolled crypto, or `rand`-based nonces.
- **Format changes need a version bump.** Any change to `vault.json` or record files needs a `HEADER_VERSION`/`RECORD_VERSION` bump and a matching update to `docs/vault-format.md`.
- **Keep AAD bindings.** Wrapped keys and records are bound to the vault id and their own id via AAD. Keep it that way.
- **Decrypted values only reach the webview through `reveal_variable`.** Listing commands return names only. Copying happens in Rust (`copy_variable`), never via the JS clipboard API.
- **Webview permissions stay minimal.** `src-tauri/capabilities/default.json` grants core, window dragging and the folder picker only. Don't grant clipboard, filesystem or shell access to the webview. Keep the strict CSP in `tauri.conf.json`.
- **Wipe key material.** Wrap passwords and key material in `Zeroizing`. `SecretKey` is zeroized on drop; don't copy its bytes into plain `Vec`s or `String`s.
- **No test backdoors.** Never add a bypass to the unlock flow (env vars, debug passwords, skip flags) for testing.

### Rust

- Match the existing style: small modules, doc comments that explain why, `thiserror` errors in core, `anyhow` in the CLI.
- **Core tests:** live in `crates/core/tests/vault.rs` and use cheap Argon2 params (`FAST`). Add a test for every format, crypto or merge change, including the tamper cases.
- **CLI tests:** run the real binary with `DOTSAFE_CONFIG_DIR`, `DOTSAFE_NO_KEYCHAIN=1` and `DOTSAFE_PASSWORD`, so they never touch the user's real config or keychain.
- **Don't run `cargo remove` blindly.** It silently deletes the `[profile.dev.package.blake2]` override in the root `Cargo.toml`. That override keeps Argon2 fast in debug builds; without it, tests get about 10× slower. Re-add it if it disappears.

### Frontend

- **Use shadcn/ui components.** Install them with `bunx --bun shadcn@latest add <name>`; they land in `src/components/ui/`. Reuse what's there before adding more. Don't hand-roll primitives (buttons, inputs, dialogs, menus), and don't put several components in one file. App-specific components go in `src/components/`, one per file.
- **Colours come from the shadcn tokens in `src/index.css`.** No hardcoded colours. `--shell` is the outer gray of cards and modals, and `--destructive-solid` is for solid delete buttons.
- **Theme:** the user picks System, Light or Dark in the sidebar's appearance menu. `src/lib/theme.ts` stores the choice, toggles `.dark` on `<html>`, and sets the native window theme. Read it with `useTheme()`, and never let theming throw during startup.
- **Cards and modals use `src/components/Surface.tsx`:** a gray shell containing a header (icon tile, title, description, optional action) and a raised inner card holding the content, with footer actions right-aligned. Don't build ad-hoc bordered panels. Buttons, inputs and tabs are pill-shaped.
- **Animations use `motion`** (`import { motion } from "motion/react"`) with the shared timings in `src/lib/motion.ts`. Keep them short and eased-out, with no bouncy springs on content. The app is wrapped in `MotionConfig reducedMotion="user"`. Any imperative animation (`useAnimate`) must also check `useReducedMotion()`.
- **Call the backend through the typed wrappers in `src/lib/api.ts`.** Errors arrive as `ApiError` with a `kind`. A `locked` error sends the UI back to the unlock screen automatically.
- **Write copy in short, active voice:** "No variables yet", "Wrong password." Every list needs loading (Skeleton), empty and error states.

## Testing the desktop app

- **Use a throwaway config.** `DOTSAFE_CONFIG_DIR=/some/tmp/dir bun run tauri dev` keeps testing away from the real config. The default vault location is the user's real iCloud Drive, so never create test vaults there.
- **Browser mode:** `bun run dev` serves the UI at http://localhost:1420 with an in-memory mock (`src/dev/mockBackend.ts`). Unlock with `password123`; add `?setup` to start at setup. The mock is dev-only and stripped from production builds. Keep it in step when command signatures change.
- **Password fields can't be automated in the native app.** macOS secure input blocks synthetic keystrokes into password fields, so unlock flows can't be driven by automation there. Use the browser mock for UI flows and say clearly what wasn't verified natively.
- **Known gap:** the desktop commands (`src-tauri/src/commands.rs`) have no automated tests yet.
