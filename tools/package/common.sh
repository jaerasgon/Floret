# shellcheck shell=bash
#
# Shared helpers for the packaging scripts in this folder (macos.sh,
# linux.sh, windows.sh). Source it; don't run it.
#
# Every script takes:  [--profile NAME] [--skip-build] [VERSION]
#   VERSION       the release version used in file names, e.g. 0.2.0-beta.1
#                 (a leading "v" is dropped). Default: the version in
#                 native/Cargo.toml.
#   --profile     the cargo profile to build and package (default: release).
#   --skip-build  package the binary already built; don't run cargo.
#
# Environment:
#   FLORET_BIN    package this binary instead of native/target/<profile>/floret
#                 (for testing a script with a stand-in).
#   DIST_DIR      where the artifacts go (default: <repo>/dist).

PKG_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$PKG_DIR/../.." && pwd)"
NATIVE="$ROOT/native"
DIST="${DIST_DIR:-$ROOT/dist}"

die() {
  echo "error: $*" >&2
  exit 1
}

# The workspace version, read the way floret.sh reads it.
cargo_version() {
  sed -n 's/^version = "\(.*\)"/\1/p' "$NATIVE/Cargo.toml" | head -1
}

PROFILE=release
SKIP_BUILD=0
VERSION=""

usage() {
  sed -n '6,16p' "$PKG_DIR/common.sh" | sed 's/^# \{0,1\}//'
}

# parse_args "$@" — fills PROFILE, SKIP_BUILD and VERSION.
parse_args() {
  while [ $# -gt 0 ]; do
    case "$1" in
      --profile)
        [ $# -ge 2 ] || die "--profile needs a value"
        PROFILE="$2"
        shift 2
        ;;
      --skip-build)
        SKIP_BUILD=1
        shift
        ;;
      -h | --help)
        usage
        exit 0
        ;;
      -*)
        die "unknown option: $1 (try --help)"
        ;;
      *)
        [ -z "$VERSION" ] || die "more than one version given: '$VERSION' and '$1'"
        VERSION="$1"
        shift
        ;;
    esac
  done
  VERSION="${VERSION#v}"
  if [ -z "$VERSION" ]; then
    VERSION="$(cargo_version)"
    [ -n "$VERSION" ] || die "could not read the version from $NATIVE/Cargo.toml"
  fi
  # The version goes into file and folder names, so keep it tame.
  case "$VERSION" in
    *[!0-9A-Za-z._+-]* | [!0-9A-Za-z]*)
      die "bad version '$VERSION': use letters, digits and . _ + - (e.g. 0.2.0-beta.1)"
      ;;
  esac
}

# The cargo profile's output folder ("dev" writes to "debug").
profile_dir() {
  case "$PROFILE" in
    dev) echo debug ;;
    *) echo "$PROFILE" ;;
  esac
}

# build_floret — cargo build of the app, unless --skip-build or FLORET_BIN.
build_floret() {
  if [ "$SKIP_BUILD" = 1 ] || [ -n "${FLORET_BIN:-}" ]; then
    echo "→ Not building (--skip-build or FLORET_BIN set)"
    return 0
  fi
  command -v cargo >/dev/null 2>&1 || die "cargo not found (install Rust from https://rustup.rs)"
  echo "→ cargo build --profile $PROFILE -p floret-app"
  (cd "$NATIVE" && cargo build --profile "$PROFILE" -p floret-app)
}

# find_binary [EXE_SUFFIX] — sets BIN to the built binary and checks it.
find_binary() {
  local suffix="${1:-}" target="${CARGO_TARGET_DIR:-$NATIVE/target}"
  case "$target" in
    /* | ?:*) ;;
    *) target="$NATIVE/$target" ;; # cargo runs in native/
  esac
  BIN="${FLORET_BIN:-$target/$(profile_dir)/floret$suffix}"
  [ -f "$BIN" ] || die "binary not found: $BIN (build it first, or drop --skip-build)"
}

# Files that travel with every build, as "source from the repo root|path
# inside the package". The first four are required; NOTICE is copied when
# it exists.
REQUIRED_LICENCE_FILES=(
  "LICENSE|LICENSE"
  "THIRD_PARTY_NOTICES.md|THIRD_PARTY_NOTICES.md"
  "native/licenses/crates.md|licenses/crates.md"
  "native/licenses/licence-texts.txt|licenses/licence-texts.txt"
)
OPTIONAL_LICENCE_FILES=(
  "NOTICE|NOTICE"
)

# require_licence_files — die if a required licence file is missing.
require_licence_files() {
  local entry src
  for entry in "${REQUIRED_LICENCE_FILES[@]}"; do
    src="${entry%%|*}"
    [ -f "$ROOT/$src" ] || die "missing $src (it must ship with every build)"
  done
}

# copy_licence_files DEST_DIR — LICENSE, NOTICE (if present),
# THIRD_PARTY_NOTICES.md and licenses/*.
copy_licence_files() {
  local dest="$1" entry src rel
  require_licence_files
  for entry in "${REQUIRED_LICENCE_FILES[@]}" "${OPTIONAL_LICENCE_FILES[@]}"; do
    src="${entry%%|*}"
    rel="${entry#*|}"
    [ -f "$ROOT/$src" ] || continue
    mkdir -p "$dest/$(dirname "$rel")"
    cp "$ROOT/$src" "$dest/$rel"
  done
}

sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d' ' -f1
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | cut -d' ' -f1
  else
    die "need sha256sum or shasum to write checksums"
  fi
}

# add_checksum FILE — put "<sha256>  <name>" for FILE into $DIST/SHA256SUMS,
# replacing an older line for the same name.
add_checksum() {
  local file="$1" name sum sums="$DIST/SHA256SUMS"
  name="$(basename "$file")"
  sum="$(sha256_of "$file")"
  touch "$sums"
  awk -v n="$name" '$2 != n' "$sums" >"$sums.tmp"
  printf '%s  %s\n' "$sum" "$name" >>"$sums.tmp"
  mv "$sums.tmp" "$sums"
  echo "  sha256 $sum  $name"
}

# stable_name FILE — the file name without the version in it:
# "Floret-0.2.0-linux-x86_64.tar.gz" becomes "Floret-linux-x86_64.tar.gz".
stable_name() {
  local name stable
  name="$(basename "$1")"
  stable="${name/"-$VERSION-"/-}"
  [ "$stable" != "$name" ] || die "cannot make a version-free name from '$name' (version $VERSION)"
  echo "$stable"
}

# add_stable_copy FILE — also write FILE under its version-free name next to
# it in $DIST, and checksum that too. The same bytes under a name that never
# changes, so a link to it never needs editing for a new release. Call it
# after FILE is final.
add_stable_copy() {
  local stable
  stable="$(stable_name "$1")"
  rm -f "$DIST/$stable"
  cp "$1" "$DIST/$stable"
  add_checksum "$DIST/$stable"
}
