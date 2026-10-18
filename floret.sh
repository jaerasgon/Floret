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

MACOS=crates/floret-app/macos
APP=target/Floret.app
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
BUILD="$(git rev-list --count HEAD)"
COMMIT="$(git rev-parse --short HEAD)"

echo "→ Making Floret.app ($VERSION, build $BUILD, $COMMIT)…"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/fast/floret "$APP/Contents/MacOS/floret"
cp "$MACOS/Info.plist" "$APP/Contents/Info.plist"
printf 'APPL????' > "$APP/Contents/PkgInfo"
PB=/usr/libexec/PlistBuddy
PLIST="$APP/Contents/Info.plist"
$PB -c "Add :CFBundleIconFile string Floret" "$PLIST"
$PB -c "Add :CFBundleShortVersionString string $VERSION" "$PLIST"
$PB -c "Add :CFBundleVersion string $BUILD" "$PLIST"
$PB -c "Add :CFBundleGetInfoString string Floret $VERSION ($COMMIT)" "$PLIST"
$PB -c "Add :LSMinimumSystemVersion string 11.0" "$PLIST"
$PB -c "Add :LSApplicationCategoryType string public.app-category.graphics-design" "$PLIST"
# Double-clicking a .flora file opens Floret: the document type and the
# app.floret.patch type come from the base Info.plist.
if ! $PB -c "Print :UTExportedTypeDeclarations:0:UTTypeIdentifier" "$PLIST" 2>/dev/null |
  grep -qx app.floret.patch; then
  echo "  (Info.plist has no .flora document type: Finder won't open patches in Floret)"
fi
plutil -lint -s "$PLIST" || echo "  (Info.plist didn't pass plutil -lint)"

# The icon: every size macOS asks for, from the 1024 px master.
ICONSET="$(mktemp -d)/Floret.iconset"
mkdir -p "$ICONSET"
for s in 16 32 128 256 512; do
  sips -z $s $s "$MACOS/icon-1024.png" --out "$ICONSET/icon_${s}x${s}.png" >/dev/null
  d=$((s * 2))
  sips -z $d $d "$MACOS/icon-1024.png" --out "$ICONSET/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/Floret.icns"
rm -rf "$(dirname "$ICONSET")"

# The licence, its NOTICE and the third-party notices travel with the app (the
# same texts are in Help > Licenses).
for f in ../LICENSE ../NOTICE ../THIRD_PARTY_NOTICES.md licenses/crates.md licenses/licence-texts.txt; do
  if [ -f "$f" ]; then
    cp "$f" "$APP/Contents/Resources/"
  else
    echo "  (missing $f: not copied into the app)"
  fi
done

# Ad-hoc signature: enough for this Mac (not for sharing the app). macOS may
# ask again for camera / microphone access after an update.
codesign --force --sign - --identifier app.floret.Floret "$APP" >/dev/null 2>&1 ||
  echo "  (codesign failed; the app should still open on this Mac)"

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
