#!/usr/bin/env sh
# Clone the JSON Schema Test Suite into research/json-schema-test-suite/ at the pinned commit.
# Run from repository root. Idempotent: safe to run repeatedly.
#
# The suite is pinned because json_schema_rs/tests/suite_xfail.json lists known failures by
# draft/file/group/test description: a moving suite would change the gate under us. To move
# to a newer suite, change SUITE_REV, run `make vendor_test_suite update_suite_xfail`, and
# review the manifest diff (and refresh `make differential`).
set -e

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$REPO_ROOT"

SUITE_URL="https://github.com/json-schema-org/JSON-Schema-Test-Suite"
SUITE_REV="583d7c62c9f5fc382530d59e0fa01481a6d10470"
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
