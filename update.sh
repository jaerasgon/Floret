#!/usr/bin/env bash
#
# update.sh — pull the latest changes and (re)install deps if needed.
#
#   ./update.sh          Pull once, install deps only if they changed.
#   ./update.sh --watch  Keep pulling automatically (checks every 15s).
#                        Use this and leave it running — whenever new work
#                        is pushed, your copy updates itself and the dev
#                        server (if running) hot-reloads in the browser.
#
set -euo pipefail

# Always operate from the folder this script lives in, no matter where it's run.
cd "$(dirname "$0")"

BRANCH="$(git rev-parse --abbrev-ref HEAD)"
INTERVAL=15

pull_once() {
  # Remember where we were so we can tell if package-lock changed.
  local before
  before="$(git rev-parse HEAD)"

  git fetch --quiet origin "$BRANCH"

  local remote
  remote="$(git rev-parse "origin/$BRANCH")"

  if [ "$before" = "$remote" ]; then
    return 1   # nothing new
  fi

  echo "→ New changes found. Pulling..."
  git merge --ff-only "origin/$BRANCH"

  # Reinstall deps only if the lockfile changed or node_modules is missing.
  if [ ! -d node_modules ] || ! git diff --quiet "$before" HEAD -- package-lock.json; then
    echo "→ Dependencies changed — running npm install..."
    npm install
  fi

  echo "✓ Up to date ($(git rev-parse --short HEAD)). Browser will hot-reload if the dev server is running."
  return 0
}

if [ "${1:-}" = "--watch" ]; then
  echo "Watching branch '$BRANCH' for updates every ${INTERVAL}s. Press Ctrl+C to stop."
  while true; do
    pull_once || true
    sleep "$INTERVAL"
  done
else
  if ! pull_once; then
    echo "✓ Already up to date ($(git rev-parse --short HEAD))."
  fi
fi
