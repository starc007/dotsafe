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
#   DOTSAFE_UPDATER_KEY           dotsafe's updater key file [default: ~/.tauri/dotsafe.key]
#   DOTSAFE_UPDATER_KEY_PASSWORD  its password (asked for if unset)
#   APPLE_SIGNING_IDENTITY        "Developer ID Application: … (TEAMID)"
#                                 [default: the keychain's, if it has exactly one]
#   APPLE_ID, APPLE_PASSWORD, APPLE_TEAM_ID
#                                 notarization (APPLE_PASSWORD is an app-specific password)
#
# The updater key has dotsafe's own names on purpose. TAURI_SIGNING_PRIVATE_KEY
# in a shell profile often belongs to another Tauri app, and an update signed
# with the wrong key is one every install refuses. So any TAURI_SIGNING_* already
# set is replaced, and the signature is checked against tauri.conf.json before
# anything is published.

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
for name in APPLE_ID APPLE_PASSWORD APPLE_TEAM_ID; do
  [[ -n "${!name:-}" ]] || fail "$name isn't set"
done

updater_key="${DOTSAFE_UPDATER_KEY:-$HOME/.tauri/dotsafe.key}"
[[ -f "$updater_key" ]] || fail "no updater key at $updater_key; set DOTSAFE_UPDATER_KEY"
if [[ -z "${DOTSAFE_UPDATER_KEY_PASSWORD+set}" ]]; then
  read -rsp "Password for $updater_key (Enter if none): " DOTSAFE_UPDATER_KEY_PASSWORD
  echo
fi
export TAURI_SIGNING_PRIVATE_KEY="$updater_key"
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD="$DOTSAFE_UPDATER_KEY_PASSWORD"

if [[ -z "${APPLE_SIGNING_IDENTITY:-}" ]]; then
  identities=$(security find-identity -v -p codesigning | sed -n 's/.*"\(Developer ID Application: .*\)"$/\1/p' | sort -u)
  count=$(printf '%s' "$identities" | grep -c . || true)
  [[ "$count" -eq 1 ]] || fail "found $count Developer ID Application identities in the keychain; set APPLE_SIGNING_IDENTITY"
  export APPLE_SIGNING_IDENTITY="$identities"
fi
echo "Signing as $APPLE_SIGNING_IDENTITY, updates with $updater_key"
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

# macOS 26 draws app icons from Assets.car and blurs an .icns-only one. Tauri
# can compile an Icon Composer icon itself, but its actool call fails with
# Xcode 27 ("Bad file descriptor"), so it's compiled here. Info.plist names it.
step "Compile the app icon (Icon Composer)"
mkdir -p "$out/icon"
xcrun actool apps/desktop/src-tauri/icons/AppIcon.icon --compile "$out/icon" \
  --platform macosx --minimum-deployment-target 11.0 --app-icon AppIcon \
  --output-partial-info-plist "$out/icon/partial.plist" --output-format human-readable-text >/dev/null
[[ -f "$out/icon/Assets.car" ]] || fail "actool didn't make Assets.car"

step "Build the app (universal, signed and notarized, with the CLI inside)"
# Only release builds carry the CLI and Assets.car, so the file mapping is
# passed here rather than living in tauri.conf.json, where `tauri dev` would
# need them too.
printf '{"bundle":{"macOS":{"files":{"MacOS/dotsafe":"%s","Resources/Assets.car":"%s"}}}}\n' \
  "$out/dotsafe" "$out/icon/Assets.car" > "$out/bundle-cli.json"
bun run tauri build --target universal-apple-darwin --config "$out/bundle-cli.json"

bundle=target/universal-apple-darwin/release/bundle
dmg=$(ls "$bundle"/dmg/*.dmg)
archive="$bundle/macos/dotsafe.app.tar.gz"
[[ -f "$archive" && -f "$archive.sig" ]] || fail "the updater archive or its signature is missing"
[[ -x "$bundle/macos/dotsafe.app/Contents/MacOS/dotsafe" ]] || fail "the CLI isn't inside the app"
[[ -f "$bundle/macos/dotsafe.app/Contents/Resources/Assets.car" ]] || fail "the app icon isn't inside the app"
bun scripts/release-tools.ts check-update-key "$archive.sig" apps/desktop/src-tauri/tauri.conf.json \
  || fail "wrong updater key; set DOTSAFE_UPDATER_KEY to dotsafe's key"
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
bun scripts/release-tools.ts latest-json "$out/latest.json" "$version" "$url" "$app.sig" ${notes:+"$notes"}

step "Publish $tag"
# An unversioned copy too, so the site's Download button can link straight to
# releases/latest/download/dotsafe.dmg.
cp "$dmg" "$out/dotsafe.dmg"
notes_args=(--generate-notes)
[[ -n "$notes" ]] && notes_args=(--notes-file "$notes")
git tag -a "$tag" -m "dotsafe $version"
git push -q origin "$tag"
gh release create "$tag" --repo "$repo" --title "dotsafe $version" "${notes_args[@]}" \
  "$dmg" "$out/dotsafe.dmg" "$app" "$app.sig" "$cli" "$out/latest.json"

printf '\n\033[1;32m✓ Released %s\033[0m https://github.com/%s/releases/tag/%s\n' "$tag" "$repo" "$tag"
