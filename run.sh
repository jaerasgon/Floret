#!/usr/bin/env bash
#
# run.sh — start the app for local testing.
#
# Installs dependencies the first time (if node_modules is missing), then
# starts the Vite dev server. Leave it running; it hot-reloads on changes.
# Open http://localhost:5173 in your browser. Press Ctrl+C to stop.
#
set -euo pipefail

cd "$(dirname "$0")"

if [ ! -d node_modules ]; then
  echo "→ First run — installing dependencies..."
  npm install
fi

echo "→ Starting dev server at http://localhost:5173 (Ctrl+C to stop)"
npm run dev
