#!/usr/bin/env bash
#
# floret.sh — pull the latest Floret and run the native app.
#
#   ./floret.sh            Pull, build (only what changed) and launch.
#   ./floret.sh --no-pull  Launch without pulling.
#
# The first run installs Rust (rustup, no Homebrew needed) and builds
# everything, which takes several minutes. After that, each run only rebuilds
# what changed: usually well under a minute.
#
set -euo pipefail
cd "$(dirname "$0")"

# Xcode command line tools provide the linker (git already needs them).
if [ "$(uname)" = "Darwin" ] && ! xcode-select -p >/dev/null 2>&1; then
  echo "→ Installing the Xcode command line tools (a dialog will open)…"
  xcode-select --install || true
  echo "  Run ./floret.sh again when that finishes."
  exit 1
fi

# Rust, installed for this user only.
if ! command -v cargo >/dev/null 2>&1; then
  if [ -x "$HOME/.cargo/bin/cargo" ]; then
    export PATH="$HOME/.cargo/bin:$PATH"
  else
    echo "→ First run: installing Rust…"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
    export PATH="$HOME/.cargo/bin:$PATH"
  fi
fi

if [ "${1:-}" != "--no-pull" ]; then
  BRANCH="$(git rev-parse --abbrev-ref HEAD)"
  echo "→ Pulling the latest '$BRANCH'…"
  git pull --ff-only origin "$BRANCH"
fi

echo "→ Building and starting Floret (first build takes a few minutes)…"
cd native
exec cargo run --profile fast -p floret-app
