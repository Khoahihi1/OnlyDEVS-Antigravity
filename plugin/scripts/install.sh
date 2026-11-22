#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DEST="${1:-${HOME}/.gemini/config/plugins/antigravity-filter-bypass}"
PYTHON="${PYTHON:-python3}"

if ! command -v "$PYTHON" >/dev/null 2>&1; then
  echo "Python not found. Set PYTHON= or install python3." >&2
  exit 1
fi

mkdir -p "$DEST"
for f in filter_bypass.py hooks.json plugin.json test_filter_bypass.py README.md CHANGELOG.md; do
  cp -f "$ROOT/$f" "$DEST/"
done

"$PYTHON" "$DEST/test_filter_bypass.py"
echo "Installed to: $DEST"