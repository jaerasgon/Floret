#!/usr/bin/env bash
#
# floret.sh — pull the latest Floret and run the native app.
#
#   ./floret.sh            Pull, build (only what changed) and launch.
#   ./floret.sh --no-pull  Launch without pulling.
#   ./floret.sh install    Pull, build and install Floret.app in Applications
#                          (macOS), then open it. Run again to update.
#   ./floret.sh install --no-pull
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

MODE=run
if [ "${1:-}" = "install" ]; then
  MODE=install
  shift
fi

if [ "${1:-}" != "--no-pull" ]; then
  BRANCH="$(git rev-parse --abbrev-ref HEAD)"
  echo "→ Pulling the latest '$BRANCH'…"
  git pull --ff-only origin "$BRANCH"
fi

cd native

if [ "$MODE" = "run" ]; then
  echo "→ Building and starting Floret (first build takes a few minutes)…"
  exec cargo run --profile fast -p floret-app
fi

# ── install: build Floret.app and put it in Applications ────────────────────
if [ "$(uname)" != "Darwin" ]; then
  echo "./floret.sh install makes a macOS app; on this system use ./floret.sh"
  exit 1
fi
echo "→ Building Floret (first build takes a few minutes)…"
cargo build --profile fast -p floret-app

APP=target/Floret.app
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
BUILD="$(git rev-list --count HEAD)"
COMMIT="$(git rev-parse --short HEAD)"

echo "→ Making Floret.app ($VERSION, build $BUILD, $COMMIT)…"
# The bundle (Info.plist, icon, licences, ad-hoc signature) is built by the
# script that release builds use too, so the two can't drift apart.
bash ../tools/package/build-macos-app.sh target/fast/floret "$APP" "$VERSION" "$BUILD" "$COMMIT"

DEST=/Applications
[ -w "$DEST" ] || DEST="$HOME/Applications"
mkdir -p "$DEST"
RUNNING=0
pgrep -xq floret && RUNNING=1
rm -rf "$DEST/Floret.app"
ditto "$APP" "$DEST/Floret.app"
# Only the installed copy, so "Open With" lists one Floret.
rm -rf "$APP"
# Make Finder / Launchpad / Spotlight pick up the new icon, version and
# .flora document type.
LSREGISTER=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister
[ -x "$LSREGISTER" ] && "$LSREGISTER" -f "$DEST/Floret.app" >/dev/null 2>&1 || true
touch "$DEST/Floret.app"

echo "✓ Installed $DEST/Floret.app"
if [ "$RUNNING" = 1 ]; then
  echo "  Floret is already open: quit it (Cmd+Q) and open it again to use this version."
else
  open "$DEST/Floret.app"
fi
