#!/usr/bin/env bash
#
# windows.sh — package Floret for Windows: a .zip in dist/.
#
#   tools/package/windows.sh [--profile NAME] [--skip-build] [VERSION]
#
# Run it in Git Bash (preinstalled on GitHub's Windows runners). Builds the
# release binary and writes
#
#   dist/Floret-<version>-windows-x86_64.zip
#   dist/SHA256SUMS                            (a line per file)
#
# The zip holds one folder, Floret-<version>/, with floret.exe, the licence
# files and a README.txt. Windows builds are experimental. See common.sh for
# the options and environment.
#
set -euo pipefail

# shellcheck source=tools/package/common.sh
. "$(dirname "${BASH_SOURCE[0]}")/common.sh"
parse_args "$@"

require_licence_files
build_floret
find_binary .exe

NAME="Floret-$VERSION-windows-x86_64"
ARCHIVE="$DIST/$NAME.zip"
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
PKG="$STAGE/Floret-$VERSION"

echo "→ Packing $NAME"
mkdir -p "$PKG" "$DIST"
cp "$BIN" "$PKG/floret.exe"
copy_licence_files "$PKG"

# Windows text files want CRLF line endings (Notepad on older Windows
# shows LF-only files as one long line).
{
  cat <<EOF
Floret $VERSION for Windows (x86_64)

EXPERIMENTAL. The Mac build is the main one; this Windows build is less
tested. Please report what doesn't work.

Run it
  1. Unzip the whole folder (right-click the zip > Extract All).
  2. Open the folder and double-click floret.exe.

Windows may show a blue "Windows protected your PC" (SmartScreen) window,
because Floret is not signed with a paid code-signing certificate. This is
normal for an unsigned app. Click "More info", then "Run anyway". You only
have to do this once for each downloaded copy. If you'd rather check first,
the SHA256SUMS.txt file next to the download lists its checksum.

If the zip was downloaded in a browser, you can also right-click floret.exe,
open Properties, tick "Unblock" at the bottom and click OK.

Floret needs a graphics driver with DirectX 12 or Vulkan (any recent GPU
driver). Patches are .flora files; drag one onto floret.exe or use
File > Open.

Licences
  LICENSE and NOTICE are Floret's licence. THIRD_PARTY_NOTICES.md and the
  files in the licenses folder list the libraries and fonts it contains.
  The same texts are in Help > Licenses inside the app.
EOF
} | sed 's/$/\r/' >"$PKG/README.txt"

rm -f "$ARCHIVE"
make_zip() {
  # 7-Zip and PowerShell are both preinstalled on windows-latest; zip is the
  # fallback (for testing this script elsewhere).
  if command -v 7z >/dev/null 2>&1; then
    (cd "$STAGE" && 7z a -tzip -bso0 -bsp0 "$1" "Floret-$VERSION")
  elif command -v 7za >/dev/null 2>&1; then
    (cd "$STAGE" && 7za a -tzip -bso0 -bsp0 "$1" "Floret-$VERSION")
  elif command -v powershell.exe >/dev/null 2>&1; then
    local src dst
    src="$(cygpath -w "$PKG")"
    dst="$(cygpath -w "$1")"
    powershell.exe -NoProfile -NonInteractive -Command \
      "Compress-Archive -Path '$src' -DestinationPath '$dst' -Force"
  elif command -v zip >/dev/null 2>&1; then
    (cd "$STAGE" && zip -qr "$1" "Floret-$VERSION")
  else
    die "no zip tool found (need 7z, PowerShell or zip)"
  fi
}
make_zip "$ARCHIVE"
[ -s "$ARCHIVE" ] || die "the zip was not created: $ARCHIVE"

echo "→ Checksums"
add_checksum "$ARCHIVE"

echo "✓ $ARCHIVE"
