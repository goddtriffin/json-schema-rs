#!/usr/bin/env bash
# Print all JSON Schema keywords from vendored draft 2020-12 meta-schemas.
# Run from repo root. Used by the competitor analysis Skill for the keyword table.
# Usage: ./research/scripts/list_keywords.sh   or   make -C research/scripts (if wired)

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
META_DIR="$REPO_ROOT/specs/json-schema.org/draft/2020-12/meta"

if [ ! -d "$META_DIR" ]; then
  echo "Meta dir not found: $META_DIR (run make vendor_specs?)" >&2
  exit 1
fi

for f in "$META_DIR"/*.json; do
  [ -f "$f" ] || continue
  if command -v jq >/dev/null 2>&1; then
    jq -r '.properties // {} | keys[]' "$f" 2>/dev/null || true
  fi
done | sort -u
