#!/usr/bin/env sh
# Clone or update the JSON Schema Test Suite into research/json-schema-test-suite/.
# Run from repository root. Idempotent: safe to run repeatedly.
set -e

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$REPO_ROOT"

SUITE_URL="https://github.com/json-schema-org/JSON-Schema-Test-Suite"
SUITE_DIR="$REPO_ROOT/research/json-schema-test-suite"

mkdir -p "$(dirname "$SUITE_DIR")"
if [ -d "$SUITE_DIR/.git" ]; then
	(cd "$SUITE_DIR" && git pull)
else
	git clone "$SUITE_URL" "$SUITE_DIR"
fi
