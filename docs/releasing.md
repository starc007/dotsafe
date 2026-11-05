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
   in `tauri.conf.json`. The release script reads the key from
   `~/.tauri/dotsafe.key` (or `DOTSAFE_UPDATER_KEY`) and asks for its password
   (or reads `DOTSAFE_UPDATER_KEY_PASSWORD`). It ignores any
   `TAURI_SIGNING_PRIVATE_KEY` in your shell, since that's often another app's
   key, and refuses to publish an update signed with a key installs don't
   expect.

2. **Signing and notarization.** A "Developer ID Application" certificate in
   the keychain (the script uses it if it's the only one; otherwise set
   `APPLE_SIGNING_IDENTITY`), and `APPLE_ID`, `APPLE_PASSWORD` (an app-specific
   password) and `APPLE_TEAM_ID`. The Apple developer agreement must be
   accepted, or notarization fails with a 403.

Keep the secrets in one environment, for example a `release` environment in a
`dotsafe` project.

3. **Linux.** Publishing a release starts `.github/workflows/release-linux.yml`,
   which builds the Linux app and CLI on GitHub Actions and adds them to the
   release. It signs updates with the same updater key, so the repo needs it
   as two Actions secrets:

   ```bash
   gh secret set DOTSAFE_UPDATER_KEY < ~/.tauri/dotsafe.key
   gh secret set DOTSAFE_UPDATER_KEY_PASSWORD
   ```

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
   `/usr/local/bin`) and the macOS 26 icon (`icons/AppIcon.icon`, compiled
   to `Assets.car` with `actool`, so Xcode 26 or later is needed), checks the
   update's signature, notarizes the standalone
   CLI download, writes `latest.json`, tags the commit and publishes the
   release, including an unversioned `dotsafe.dmg` that the site's Download
   button links to. Without a notes file, GitHub generates the notes.
3. The Linux workflow starts as soon as the release is published and takes
   about 15 minutes. It builds the AppImage, the .deb (with the CLI as
   `/usr/bin/dotsafe`) and a CLI tarball from the same tag, checks the update
   signature's key, uploads them with unversioned `dotsafe.AppImage` and
   `dotsafe.deb` copies (`dotsafe-aarch64.AppImage` and `dotsafe-arm64.deb`
   for arm64, built on GitHub's ARM runners), and adds `linux-<arch>` and
   `linux-<arch>-deb` for both architectures to the
   release's `latest.json`. Until it finishes, Linux installs see no update.
   If it fails, fix it and re-run the job; it overwrites what it uploaded.

Updates only work once the repo is public: the app downloads release assets
without signing in.
