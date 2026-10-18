#!/usr/bin/env bash
#
# linux.sh — package Floret for Linux: a .tar.gz in dist/.
#
#   tools/package/linux.sh [--profile NAME] [--skip-build] [VERSION]
#
# Builds the release binary and writes
#
#   dist/Floret-<version>-linux-x86_64.tar.gz
#   dist/SHA256SUMS                           (a line per file)
#
# The archive holds one folder, Floret-<version>/, with the binary, the
# licence files, floret.desktop, the icon and a README.txt. Linux builds are
# experimental. See common.sh for the options and environment.
#
set -euo pipefail

# shellcheck source=tools/package/common.sh
. "$(dirname "${BASH_SOURCE[0]}")/common.sh"
parse_args "$@"

[ "$(uname -s)" = "Linux" ] || die "linux.sh must run on Linux"
case "$(uname -m)" in
  x86_64 | amd64) ARCH=x86_64 ;;
  *) die "only x86_64 Linux builds are packaged (this machine is $(uname -m))" ;;
esac
command -v tar >/dev/null 2>&1 || die "tar not found"
command -v gzip >/dev/null 2>&1 || die "gzip not found"

require_licence_files
ICON="$NATIVE/crates/floret-app/macos/icon-1024.png"
[ -f "$ICON" ] || die "icon not found: $ICON"
build_floret
find_binary

NAME="Floret-$VERSION-linux-$ARCH"
ARCHIVE="$DIST/$NAME.tar.gz"
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
PKG="$STAGE/Floret-$VERSION"

echo "→ Packing $NAME"
mkdir -p "$PKG" "$DIST"
install -m 755 "$BIN" "$PKG/floret"
copy_licence_files "$PKG"
install -m 644 "$ICON" "$PKG/floret.png"

# The app id is "floret" (see main.rs), which StartupWMClass matches so the
# window groups under this launcher.
cat >"$PKG/floret.desktop" <<'EOF'
[Desktop Entry]
Type=Application
Name=Floret
GenericName=Generative art
Comment=Node-based generative art
Exec=floret %f
Icon=floret
Terminal=false
Categories=Graphics;
StartupWMClass=floret
EOF
chmod 644 "$PKG/floret.desktop"

cat >"$PKG/README.txt" <<EOF
Floret $VERSION for Linux ($ARCH)

EXPERIMENTAL. The Mac build is the main one; this Linux build is less
tested. Please report what doesn't work.

What you need
  - A 64-bit Linux desktop (X11 or Wayland) with a working graphics driver
    (Vulkan or OpenGL; on Debian/Ubuntu: mesa-vulkan-drivers or your GPU's
    driver).
  - The ALSA sound library. On Debian/Ubuntu:  sudo apt install libasound2
    (the package is called libasound2t64 on newer releases).

Run it
  Unpack the archive, then from this folder:

      ./floret                   start Floret
      ./floret my-patch.flora    start Floret with a patch

Add it to your applications menu (optional)
  From this folder:

      install -Dm755 floret ~/.local/bin/floret
      install -Dm644 floret.png ~/.local/share/icons/hicolor/1024x1024/apps/floret.png
      install -Dm644 floret.desktop ~/.local/share/applications/floret.desktop

  ~/.local/bin must be on your PATH for the launcher to find "floret".
  Opening .flora files from your file manager is not set up by this.

Licences
  LICENSE and NOTICE are Floret's licence. THIRD_PARTY_NOTICES.md and the
  files in licenses/ list the libraries and fonts it contains. The same
  texts are in Help > Licenses inside the app.
EOF

# Fixed owner, sorted names: the same input gives the same archive layout.
tar --sort=name --owner=0 --group=0 --numeric-owner -C "$STAGE" -czf "$ARCHIVE" "Floret-$VERSION"

echo "→ Checksums"
add_checksum "$ARCHIVE"

echo "✓ $ARCHIVE"
