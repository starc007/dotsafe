#!/usr/bin/env bash
# Builds, signs and publishes a dotsafe release on GitHub, from this Mac
# (GitHub Actions is off). The app's updater reads latest.json from the newest
# release, so publishing one is what ships an update.
#
#   scripts/release.sh 0.2.0 [notes.md]
#
# Bump the version first, in a PR: Cargo.toml ([workspace.package]),
# apps/desktop/src-tauri/tauri.conf.json and apps/desktop/package.json. This
# script checks they match and that main is clean and pushed.
#
# Secrets come from the environment. Keep them in dotsafe and run:
#   dotsafe run -e release -- scripts/release.sh 0.2.0
#
#   TAURI_SIGNING_PRIVATE_KEY           updater key (contents or a path), from `tauri signer generate`
#   TAURI_SIGNING_PRIVATE_KEY_PASSWORD  its password
#   APPLE_SIGNING_IDENTITY              "Developer ID Application: … (TEAMID)"
#   APPLE_ID, APPLE_PASSWORD, APPLE_TEAM_ID
#                                       notarization (APPLE_PASSWORD is an app-specific password)

set -euo pipefail

cd "$(dirname "$0")/.."

version="${1:-}"
notes="${2:-}"
if [[ ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "usage: scripts/release.sh <version, e.g. 0.2.0> [notes.md]" >&2
  exit 2
fi
tag="v$version"
repo="starc007/dotsafe"

step() {
  printf '\n\033[1m==> %s\033[0m\n' "$*"
}
fail() {
  echo "release: $*" >&2
  exit 1
}

step "Checks"
for name in TAURI_SIGNING_PRIVATE_KEY TAURI_SIGNING_PRIVATE_KEY_PASSWORD APPLE_SIGNING_IDENTITY APPLE_ID APPLE_PASSWORD APPLE_TEAM_ID; do
  [[ -n "${!name:-}" ]] || fail "$name isn't set"
done
[[ -z "$notes" || -f "$notes" ]] || fail "$notes doesn't exist"
[[ "$(git branch --show-current)" == main ]] || fail "release from main"
[[ -z "$(git status --porcelain)" ]] || fail "the working tree isn't clean"
git fetch -q origin main
[[ "$(git rev-parse HEAD)" == "$(git rev-parse origin/main)" ]] || fail "main isn't the same as origin/main"
git rev-parse -q --verify "refs/tags/$tag" >/dev/null && fail "$tag already exists"
gh auth status >/dev/null 2>&1 || fail "gh isn't signed in"

cargo_version=$(sed -n '/^\[workspace.package\]/,/^\[/s/^version = "\(.*\)"/\1/p' Cargo.toml)
tauri_version=$(sed -n 's/^  "version": "\(.*\)",/\1/p' apps/desktop/src-tauri/tauri.conf.json)
ui_version=$(sed -n 's/^  "version": "\(.*\)",/\1/p' apps/desktop/package.json)
for v in "$cargo_version" "$tauri_version" "$ui_version"; do
  [[ "$v" == "$version" ]] || fail "versions are Cargo $cargo_version, tauri.conf $tauri_version, package.json $ui_version; bump them all to $version first"
done
grep -q '"pubkey": ""' apps/desktop/src-tauri/tauri.conf.json && fail "the updater public key isn't in tauri.conf.json"

scripts/check.sh

rustup target add aarch64-apple-darwin x86_64-apple-darwin >/dev/null
out=$(mktemp -d)

# The CLI is built and signed first, because it ships inside the app too
# (Contents/MacOS/dotsafe, which Settings → Agents links into /usr/local/bin).
# Signed with the hardened runtime, it passes the app's notarization as is.
step "Build the CLI (universal, signed)"
cargo build --release --locked -p dotsafe-cli --target aarch64-apple-darwin
cargo build --release --locked -p dotsafe-cli --target x86_64-apple-darwin
lipo -create -output "$out/dotsafe" target/aarch64-apple-darwin/release/dotsafe target/x86_64-apple-darwin/release/dotsafe
codesign --force --options runtime --timestamp --sign "$APPLE_SIGNING_IDENTITY" "$out/dotsafe"

step "Build the app (universal, signed and notarized, with the CLI inside)"
# Only release builds carry the CLI, so the file mapping is passed here rather
# than living in tauri.conf.json, where `tauri dev` would need the binary too.
printf '{"bundle":{"macOS":{"files":{"MacOS/dotsafe":"%s"}}}}\n' "$out/dotsafe" > "$out/bundle-cli.json"
bun run tauri build --target universal-apple-darwin --config "$out/bundle-cli.json"

bundle=target/universal-apple-darwin/release/bundle
dmg=$(ls "$bundle"/dmg/*.dmg)
archive="$bundle/macos/dotsafe.app.tar.gz"
[[ -f "$archive" && -f "$archive.sig" ]] || fail "the updater archive or its signature is missing"
[[ -x "$bundle/macos/dotsafe.app/Contents/MacOS/dotsafe" ]] || fail "the CLI isn't inside the app"
codesign --verify --strict --deep "$bundle/macos/dotsafe.app" || fail "the app's signature doesn't verify"

# The standalone download, for people who want the CLI without the app. A bare
# binary can't be stapled, so Gatekeeper checks its notarization online.
step "Notarize the standalone CLI"
ditto -c -k --keepParent "$out/dotsafe" "$out/dotsafe-cli.zip"
xcrun notarytool submit "$out/dotsafe-cli.zip" --apple-id "$APPLE_ID" --password "$APPLE_PASSWORD" --team-id "$APPLE_TEAM_ID" --wait
cli="$out/dotsafe-$version-macos-universal.tar.gz"
tar -czf "$cli" -C "$out" dotsafe

step "Write latest.json"
app="$out/dotsafe-$version-universal.app.tar.gz"
cp "$archive" "$app"
cp "$archive.sig" "$app.sig"
url="https://github.com/$repo/releases/download/$tag/$(basename "$app")"
signature=$(cat "$app.sig")
notes_text=""
[[ -n "$notes" ]] && notes_text=$(cat "$notes")
# The universal build serves both Apple silicon and Intel Macs.
NOTES="$notes_text" SIG="$signature" URL="$url" VERSION="$version" bun -e '
  const platform = { signature: process.env.SIG, url: process.env.URL };
  const latest = {
    version: process.env.VERSION,
    notes: process.env.NOTES,
    pub_date: new Date().toISOString(),
    platforms: { "darwin-aarch64": platform, "darwin-x86_64": platform },
  };
  await Bun.write(process.argv[1], JSON.stringify(latest, null, 2) + "\n");
' "$out/latest.json"

step "Publish $tag"
notes_args=(--generate-notes)
[[ -n "$notes" ]] && notes_args=(--notes-file "$notes")
git tag -a "$tag" -m "dotsafe $version"
git push -q origin "$tag"
gh release create "$tag" --repo "$repo" --title "dotsafe $version" "${notes_args[@]}" \
  "$dmg" "$app" "$app.sig" "$cli" "$out/latest.json"

printf '\n\033[1;32m✓ Released %s\033[0m https://github.com/%s/releases/tag/%s\n' "$tag" "$repo" "$tag"
