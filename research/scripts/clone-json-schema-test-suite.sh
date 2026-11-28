#!/usr/bin/env sh
# Clone the JSON Schema Test Suite into research/json-schema-test-suite/ at the pinned commit.
# Run from repository root. Idempotent: safe to run repeatedly.
#
# The repository tracks the latest suite (and the latest Bowtie, which runs it in `make
# differential`) and adapts when they change. It is still pinned, so the known failures in
# json_schema_rs/tests/suite_xfail*.json (by draft/file/group/test description) never move under
# us: to take a newer suite, set SUITE_REV to its latest commit, run `make vendor_test_suite
# update_suite_xfail`, review the manifest diff, and refresh `make differential`.
set -e

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$REPO_ROOT"

SUITE_URL="https://github.com/json-schema-org/JSON-Schema-Test-Suite"
SUITE_REV="e2a68a15cd490750fd6b95e604e4054d7d978d1f"
SUITE_DIR="$REPO_ROOT/research/json-schema-test-suite"

mkdir -p "$(dirname "$SUITE_DIR")"
if [ ! -d "$SUITE_DIR/.git" ]; then
	git clone "$SUITE_URL" "$SUITE_DIR"
fi
cd "$SUITE_DIR"
if ! git cat-file -e "${SUITE_REV}^{commit}" 2>/dev/null; then
	git fetch origin
fi
git checkout --quiet --detach "$SUITE_REV"
echo "JSON Schema Test Suite at $(git rev-parse --short HEAD) in $SUITE_DIR"
