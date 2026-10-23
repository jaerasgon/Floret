#!/usr/bin/env bash
#
# release-notes.sh — write the GitHub release notes to standard output.
#
#   tools/package/release-notes.sh TAG ARTIFACT_DIR
#
# The notes are: a download table for the files found in ARTIFACT_DIR, how to
# open an unsigned app on each system, a note that the Windows and Linux
# builds are experimental, a rule line, then the body of docs/release-notes.md
# (its first heading is dropped: the release title says it). The notes file
# must therefore not repeat the download or first-launch steps. Used by
# .github/workflows/release.yml.
#
set -euo pipefail

[ $# -eq 2 ] || {
  echo "usage: release-notes.sh TAG ARTIFACT_DIR" >&2
  exit 2
}
TAG="$1"
DIR="$2"
VERSION="${TAG#v}"
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
NOTES="$ROOT/docs/release-notes.md"
[ -d "$DIR" ] || {
  echo "error: not a folder: $DIR" >&2
  exit 1
}
[ -f "$NOTES" ] || {
  echo "error: missing $NOTES" >&2
  exit 1
}

mac_arm="Floret-$VERSION-macos-arm64"
mac_x64="Floret-$VERSION-macos-x86_64"
win="Floret-$VERSION-windows-x86_64"
lin="Floret-$VERSION-linux-x86_64"

have() { [ -f "$DIR/$1" ]; }

# row LABEL "FILE FILE…" NOTE — a table row for the files that exist; the
# row says "not built in this run" when none does.
row() {
  local label="$1" files="$2" note="$3" f found="" out=""
  for f in $files; do
    if have "$f"; then
      out="${out:+$out, }\`$f\`"
      found=1
    fi
  done
  if [ -n "$found" ]; then
    printf '| %s | %s | %s |\n' "$label" "$out" "$note"
  else
    printf '| %s | not built in this run | |\n' "$label"
  fi
}

cat <<EOF
## Downloads

This is a **beta**: it is for trying Floret and telling me what breaks. The
apps are **not signed**, so each system asks you to confirm the first launch
(steps below).

| System | Download | Notes |
| --- | --- | --- |
EOF
row "macOS, Apple silicon (M1 or later)" "$mac_arm.dmg $mac_arm.zip" "Main platform. Needs macOS 11 or later."
row "macOS, Intel" "$mac_x64.dmg $mac_x64.zip" "Needs macOS 11 or later."
row "Windows 10 / 11, 64-bit" "$win.zip" "**Experimental.**"
row "Linux, 64-bit (x86_64)" "$lin.tar.gz" "**Experimental.**"
cat <<EOF

Each file is also attached under a name without the version number (for
example \`Floret-macos-arm64.dmg\`): the same bytes, with a name that stays
the same from release to release.

\`SHA256SUMS.txt\` lists the checksum of every file, to check a download
(\`shasum -a 256 -c SHA256SUMS.txt\` on macOS, \`sha256sum -c SHA256SUMS.txt\`
on Linux, \`Get-FileHash <file>\` in Windows PowerShell).

> **Windows and Linux builds are experimental.** The Mac build is the main
> target. The Windows and Linux builds have had far less testing: expect
> rough edges, and please report what you find.

### First launch (unsigned app)

**macOS.** Open the \`.dmg\` and drag Floret into Applications (or unzip the
\`.zip\`). Open Floret. macOS says it cannot verify the app: click Done, open
**System Settings → Privacy & Security**, scroll down, click **Open Anyway**
next to Floret, and confirm. (On macOS 14 and earlier: right-click Floret,
choose Open, then Open.) Or, in Terminal, remove the download mark first:

\`\`\`
xattr -dr com.apple.quarantine /Applications/Floret.app
\`\`\`

**Windows.** Unzip the whole folder and double-click \`floret.exe\`. If a blue
"Windows protected your PC" window appears, click **More info**, then
**Run anyway**.

**Linux.** Unpack the archive and run \`./floret\` from the folder. You need
the ALSA library (\`sudo apt install libasound2\`) and a graphics driver with
Vulkan or OpenGL. \`README.txt\` inside the archive shows how to add Floret
to your applications menu.

---

EOF
# The notes' body, without its first "# " heading line.
awk 'BEGIN { skipped = 0 } !skipped && /^# / { skipped = 1; next } { print }' "$NOTES"
