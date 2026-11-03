# Releasing

Releases are GitHub releases of this repo. The desktop app checks
`https://github.com/starc007/dotsafe/releases/latest/download/latest.json`
(at launch and every 6 hours, release builds only), shows "Update available",
and downloads only when the user clicks. The updater checks every download's
signature against the public key in `apps/desktop/src-tauri/tauri.conf.json`
before installing it.

## One-time setup

1. **Updater key.** Generate it once and keep the private key safe. Losing it
   means existing installs can't verify new updates.

   ```bash
   bun run tauri signer generate -w ~/.tauri/dotsafe.key
   ```

   Put the contents of `~/.tauri/dotsafe.key.pub` in `plugins.updater.pubkey`
   in `tauri.conf.json`. Store the private key and its password in dotsafe as
   `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.

2. **Signing and notarization.** A "Developer ID Application" certificate in
   the keychain, and in dotsafe: `APPLE_SIGNING_IDENTITY`, `APPLE_ID`,
   `APPLE_PASSWORD` (an app-specific password) and `APPLE_TEAM_ID`.

Keep these in one environment, for example a `release` environment in a
`dotsafe` project.

## Each release

1. In a PR, bump the version in `Cargo.toml` (`[workspace.package]`),
   `apps/desktop/src-tauri/tauri.conf.json` and `apps/desktop/package.json`,
   add the release to the site's changelog in `apps/web/lib/changelog.ts`
   (with its `date`), and merge it.
2. On an up-to-date, clean `main`:

   ```bash
   dotsafe run -e release -- scripts/release.sh 0.2.0 notes.md
   ```

   It runs `scripts/check.sh`, builds and signs the universal CLI, builds a
   universal, signed and notarized app with that CLI inside
   (`Contents/MacOS/dotsafe`, which Settings → Agents links into
   `/usr/local/bin`), notarizes the standalone CLI download, writes
   `latest.json`, tags the commit and publishes the release. Without a notes
   file, GitHub generates the notes.

Updates only work once the repo is public: the app downloads release assets
without signing in.
