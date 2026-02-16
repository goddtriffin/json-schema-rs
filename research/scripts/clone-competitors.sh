#!/usr/bin/env sh
# Clone or update competitor repos into research/repos/<lang>/<name>/.
# Run from repository root. Idempotent: safe to run repeatedly.
set -e

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$REPO_ROOT"

RUST_REPOS="$REPO_ROOT/research/repos/rust"
mkdir -p "$RUST_REPOS"

# Git competitors: clone or pull
git_clone_or_pull() {
	dir="$1"
	url="$2"
	if [ -d "$RUST_REPOS/$dir/.git" ]; then
		(cd "$RUST_REPOS/$dir" && git pull)
	else
		git clone "$url" "$RUST_REPOS/$dir"
	fi
}

git_clone_or_pull "Stranger6667-jsonschema-rs" "https://github.com/Stranger6667/jsonschema-rs"
git_clone_or_pull "Marwes-schemafy" "https://github.com/Marwes/schemafy"
git_clone_or_pull "oxidecomputer-typify" "https://github.com/oxidecomputer/typify"

# BelfordZ json_schema: crates.io tarball (no Git repo)
# Set REFRESH=1 to force re-download (script removes existing dir automatically).
JSON_SCHEMA_CRATE_VERSION="1.8.0"
BELFORDZ_DIR="$RUST_REPOS/BelfordZ-json-schema"
BELFORDZ_CARGO="$BELFORDZ_DIR/Cargo.toml"

if [ -n "${REFRESH-}" ]; then
	rm -rf "$BELFORDZ_DIR"
fi

if [ -f "$BELFORDZ_CARGO" ] && grep -q 'name = "json_schema"' "$BELFORDZ_CARGO" && grep -q "version = \"$JSON_SCHEMA_CRATE_VERSION\"" "$BELFORDZ_CARGO"; then
	: # already present and correct, skip
else
	CRATE_URL="https://crates.io/api/v1/crates/json_schema/$JSON_SCHEMA_CRATE_VERSION/download"
	TARBALL="$RUST_REPOS/json_schema-$JSON_SCHEMA_CRATE_VERSION.tar.gz"
	EXTRACTED="$RUST_REPOS/json_schema-$JSON_SCHEMA_CRATE_VERSION"

	curl -sL "$CRATE_URL" -o "$TARBALL"
	tar -xzf "$TARBALL" -C "$RUST_REPOS"
	rm -rf "$BELFORDZ_DIR"
	mkdir -p "$BELFORDZ_DIR"
	( cd "$EXTRACTED" && tar cf - . ) | ( cd "$BELFORDZ_DIR" && tar xf - )
	rm -rf "$EXTRACTED" "$TARBALL"
fi
