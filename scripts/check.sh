#!/usr/bin/env bash
# Runs everything CI ran (.github/workflows/ci.yml), on this machine. Run it
# before pushing; GitHub Actions is switched off for this repo.
#
#   scripts/check.sh          everything
#   scripts/check.sh rust     frontend build, clippy, Rust tests
#   scripts/check.sh api      API typecheck and tests
#   scripts/check.sh web      site and docs typecheck
#
# It covers this OS only. CI also built on Linux and Windows, so keep
# platform-specific code (cfg(unix), paths, permissions) obviously correct.

set -euo pipefail

cd "$(dirname "$0")/.."

what="${1:-all}"
case "$what" in
  all | rust | api | web) ;;
  *)
    echo "usage: scripts/check.sh [all|rust|api|web]" >&2
    exit 2
    ;;
esac

step() {
  printf '\n\033[1m==> %s\033[0m\n' "$*"
}

started=$SECONDS

if [[ "$what" == all || "$what" == rust ]]; then
  step "Toolchain"
  rustc --version
  cargo clippy --version
  bun --version

  # The desktop crate embeds the built frontend (apps/desktop/dist) at compile time.
  step "Frontend: install and build"
  bun install --frozen-lockfile
  rm -rf apps/desktop/dist
  bun run build
  # A wrong bun invocation prints its help and exits 0, so check it built.
  test -f apps/desktop/dist/index.html || { echo "the frontend didn't build" >&2; exit 1; }

  step "Clippy"
  cargo clippy --workspace --all-targets --locked -- -D warnings

  step "Rust tests"
  cargo test --workspace --locked
fi

if [[ "$what" == all || "$what" == api ]]; then
  step "API: install, typecheck, tests"
  bun install --frozen-lockfile
  (
    cd apps/api
    bun run typecheck
    bun run test
  )
fi

if [[ "$what" == all || "$what" == web ]]; then
  step "Web: install, typecheck"
  bun install --frozen-lockfile
  bun run --cwd apps/web types:check
fi

printf '\n\033[1;32m✓ All checks passed\033[0m (%ss)\n' "$((SECONDS - started))"
