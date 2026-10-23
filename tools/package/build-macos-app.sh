#!/usr/bin/env bash
#
# build-macos-app.sh — make Floret.app from a built binary.
#
# The one place that knows what goes into the bundle. Used by
# `./floret.sh install` (installs the result) and by tools/package/macos.sh
# (zips it and makes a .dmg for a release).
#
#   build-macos-app.sh BINARY APP VERSION BUILD COMMIT [LABEL]
#
#     BINARY   the built `floret` executable
#     APP      the bundle to create (an existing one is replaced)
#     VERSION  the app version, e.g. 0.2.0 or 0.2.0-beta.1. Apple wants digits
#              only in CFBundleShortVersionString, so a pre-release suffix
#              (everything from the first - or +) is cut off there.
#     BUILD    CFBundleVersion, e.g. the commit count
#     COMMIT   short commit hash, shown in Get Info
#     LABEL    version text for Get Info (default: VERSION), e.g. 0.2.0-beta.1
#
# The bundle gets the icon, the version keys, the licence files and an
# ad-hoc signature. It runs on macOS only (sips, iconutil, PlistBuddy,
# codesign).
#
set -euo pipefail

if [ $# -lt 5 ] || [ $# -gt 6 ]; then
  echo "usage: build-macos-app.sh BINARY APP VERSION BUILD COMMIT [LABEL]" >&2
  exit 2
fi
BIN="$1"
APP="$2"
VERSION="$3"
BUILD="$4"
COMMIT="$5"
LABEL="${6:-$VERSION}"
SHORT_VERSION="${VERSION%%[-+]*}" # 0.2.0-beta.1 -> 0.2.0

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
MACOS="$ROOT/native/crates/floret-app/macos"

[ -f "$BIN" ] || {
  echo "error: binary not found: $BIN" >&2
  exit 1
}

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/floret"
cp "$MACOS/Info.plist" "$APP/Contents/Info.plist"
printf 'APPL????' >"$APP/Contents/PkgInfo"
PB=/usr/libexec/PlistBuddy
PLIST="$APP/Contents/Info.plist"
$PB -c "Add :CFBundleIconFile string Floret" "$PLIST"
$PB -c "Add :CFBundleShortVersionString string $SHORT_VERSION" "$PLIST"
$PB -c "Add :CFBundleVersion string $BUILD" "$PLIST"
$PB -c "Add :CFBundleGetInfoString string Floret $LABEL ($COMMIT)" "$PLIST"
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

# The licence and the third-party notices travel with the app (the same texts
# are in Help → Licenses). NOTICE is copied when the repository has one.
for f in LICENSE NOTICE THIRD_PARTY_NOTICES.md native/licenses/crates.md native/licenses/licence-texts.txt; do
  if [ -f "$ROOT/$f" ]; then
    cp "$ROOT/$f" "$APP/Contents/Resources/"
  elif [ "$f" != NOTICE ]; then
    echo "  (missing $f: not copied into the app)"
  fi
done

# Ad-hoc signature: enough for this Mac (not for sharing the app). macOS may
# ask again for camera / microphone access after an update.
codesign --force --sign - --identifier app.floret.Floret "$APP" >/dev/null 2>&1 ||
  echo "  (codesign failed; the app should still open on this Mac)"
