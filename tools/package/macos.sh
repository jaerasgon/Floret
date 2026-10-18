#!/usr/bin/env bash
#
# macos.sh — package Floret for macOS: a .zip and a .dmg in dist/.
#
#   tools/package/macos.sh [--profile NAME] [--skip-build] [VERSION]
#
# Builds the release binary, makes Floret.app (the same bundle that
# `./floret.sh install` makes, from tools/package/build-macos-app.sh),
# ad-hoc signs it, and writes
#
#   dist/Floret-<version>-macos-<arch>.zip   (made with ditto, bundle intact)
#   dist/Floret-<version>-macos-<arch>.dmg   (with an /Applications shortcut)
#   dist/SHA256SUMS                          (a line per file)
#
# Both contain a "READ ME FIRST.txt" about opening an unsigned app. <arch> is
# arm64 or x86_64, taken from the machine it runs on (override: FLORET_ARCH).
# Run it on a Mac. See common.sh for the options and environment.
#
set -euo pipefail

# shellcheck source=tools/package/common.sh
. "$(dirname "${BASH_SOURCE[0]}")/common.sh"
parse_args "$@"

[ "$(uname)" = "Darwin" ] || die "macos.sh must run on macOS (it uses ditto, hdiutil, codesign)"
for tool in ditto hdiutil codesign sips iconutil plutil /usr/libexec/PlistBuddy; do
  command -v "$tool" >/dev/null 2>&1 || [ -x "$tool" ] || die "required macOS tool not found: $tool"
done

ARCH="${FLORET_ARCH:-$(uname -m)}"
case "$ARCH" in
  arm64 | aarch64) ARCH=arm64 ;;
  x86_64 | amd64) ARCH=x86_64 ;;
  *) die "unexpected CPU architecture: $ARCH" ;;
esac

require_licence_files
build_floret
find_binary

SHORT_VERSION="$(cargo_version)" # the bundle's CFBundleShortVersionString (numeric)
BUILD="${FLORET_BUILD:-$(git -C "$ROOT" rev-list --count HEAD 2>/dev/null || echo 1)}"
COMMIT="$(git -C "$ROOT" rev-parse --short HEAD 2>/dev/null || echo unknown)"
NAME="Floret-$VERSION-macos-$ARCH"

mkdir -p "$DIST"
rm -f "$DIST/$NAME.zip" "$DIST/$NAME.dmg"
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT

echo "→ Making Floret.app ($VERSION, $ARCH, build $BUILD, $COMMIT)…"
APP="$STAGE/Floret.app"
bash "$PKG_DIR/build-macos-app.sh" "$BIN" "$APP" "$SHORT_VERSION" "$BUILD" "$COMMIT" "$VERSION"
codesign --verify --deep --strict "$APP" || die "the app's signature did not verify"
[ -f "$APP/Contents/Resources/LICENSE" ] || die "LICENSE did not make it into the app"

write_read_me() {
  cat >"$1" <<EOF
Floret $VERSION for macOS ($ARCH)

This is a beta. It is not signed with an Apple Developer ID, so macOS asks
before it opens Floret the first time. This is normal for an unsigned app.

1. Drag Floret.app into your Applications folder.
2. Try to open it. macOS will say it can't be opened, or that it can't
   check it for malicious software. Click Done (or Cancel).
3. Open System Settings > Privacy & Security, scroll down to the message
   about "Floret", and click "Open Anyway". Confirm, and Floret opens.
   (On macOS 14 and earlier you can instead right-click Floret.app, choose
   Open, then Open again.)

If you'd rather use Terminal, this removes the download mark and Floret
then opens normally:

   xattr -dr com.apple.quarantine /Applications/Floret.app

You only have to do this once for each downloaded copy.

Needs macOS 11 or later. Licences: LICENSE, NOTICE and
THIRD_PARTY_NOTICES.md are inside the app (Contents/Resources) and under
Help > Licenses.
EOF
}

# ── zip: a folder holding the app and the note, kept intact by ditto ────────
echo "→ Zip…"
ZIPDIR="$STAGE/zip/$NAME"
mkdir -p "$ZIPDIR"
ditto "$APP" "$ZIPDIR/Floret.app"
write_read_me "$ZIPDIR/READ ME FIRST.txt"
ditto -c -k --keepParent "$ZIPDIR" "$DIST/$NAME.zip"

# ── dmg: the app, an Applications shortcut and the note ─────────────────────
echo "→ Disk image…"
DMGDIR="$STAGE/dmg"
mkdir -p "$DMGDIR"
ditto "$APP" "$DMGDIR/Floret.app"
ln -s /Applications "$DMGDIR/Applications"
write_read_me "$DMGDIR/READ ME FIRST.txt"
# hdiutil sometimes fails with "Resource busy" on CI runners; try again.
tries=0
until hdiutil create -volname "Floret" -srcfolder "$DMGDIR" -ov -format UDZO "$DIST/$NAME.dmg" >/dev/null; do
  tries=$((tries + 1))
  [ "$tries" -lt 4 ] || die "hdiutil could not create the disk image"
  echo "  (hdiutil failed, trying again in 5 s)"
  sleep 5
  rm -f "$DIST/$NAME.dmg"
done

echo "→ Checksums"
add_checksum "$DIST/$NAME.zip"
add_checksum "$DIST/$NAME.dmg"

echo "✓ $DIST/$NAME.zip"
echo "✓ $DIST/$NAME.dmg"
