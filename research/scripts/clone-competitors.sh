#!/usr/bin/env sh
# Clone or update competitor repos into research/repos/<lang>/<name>/.
# Run from repository root. Idempotent: safe to run repeatedly.
set -e

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$REPO_ROOT"

# Git competitors: clone or pull (lang, dir, url)
git_clone_or_pull() {
	lang="$1"
	dir="$2"
	url="$3"
	LANG_REPOS="$REPO_ROOT/research/repos/$lang"
	mkdir -p "$LANG_REPOS"
	if [ -d "$LANG_REPOS/$dir/.git" ]; then
		(cd "$LANG_REPOS/$dir" && git pull)
	else
		git clone "$url" "$LANG_REPOS/$dir"
	fi
}

# Rust
git_clone_or_pull "rust" "Stranger6667-jsonschema-rs" "https://github.com/Stranger6667/jsonschema-rs"
git_clone_or_pull "rust" "Marwes-schemafy" "https://github.com/Marwes/schemafy"
git_clone_or_pull "rust" "oxidecomputer-typify" "https://github.com/oxidecomputer/typify"

# BelfordZ json_schema: crates.io tarball (no Git repo)
# Set REFRESH=1 to force re-download (script removes existing dir automatically).
RUST_REPOS="$REPO_ROOT/research/repos/rust"
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

# Python
git_clone_or_pull "python" "python-jsonschema-jsonschema" "https://github.com/python-jsonschema/jsonschema"
git_clone_or_pull "python" "koxudaxi-datamodel-code-generator" "https://github.com/koxudaxi/datamodel-code-generator"
git_clone_or_pull "python" "horejsek-python-fastjsonschema" "https://github.com/horejsek/python-fastjsonschema"
git_clone_or_pull "python" "pydantic-pydantic" "https://github.com/pydantic/pydantic"

# Go
git_clone_or_pull "go" "santhosh-tekuri-jsonschema" "https://github.com/santhosh-tekuri/jsonschema"
git_clone_or_pull "go" "omissis-go-jsonschema" "https://github.com/omissis/go-jsonschema"
git_clone_or_pull "go" "xeipuuv-gojsonschema" "https://github.com/xeipuuv/gojsonschema"

# Java
git_clone_or_pull "java" "networknt-json-schema-validator" "https://github.com/networknt/json-schema-validator"
git_clone_or_pull "java" "joelittlejohn-jsonschema2pojo" "https://github.com/joelittlejohn/jsonschema2pojo"
git_clone_or_pull "java" "java-json-tools-json-schema-validator" "https://github.com/java-json-tools/json-schema-validator"

# TypeScript (quicktype is shared multi-language generator)
git_clone_or_pull "typescript" "ajv-validator-ajv" "https://github.com/ajv-validator/ajv"
git_clone_or_pull "typescript" "bcherny-json-schema-to-typescript" "https://github.com/bcherny/json-schema-to-typescript"
git_clone_or_pull "typescript" "glideapps-quicktype" "https://github.com/glideapps/quicktype"

# C
git_clone_or_pull "c" "netmail-open-wjelement" "https://github.com/netmail-open/wjelement"
git_clone_or_pull "c" "badicsalex-json_schema_to_c" "https://github.com/badicsalex/json_schema_to_c"
git_clone_or_pull "c" "helmut-jacob-jsonschema-c" "https://github.com/helmut-jacob/jsonschema-c"

# C++
git_clone_or_pull "cpp" "sourcemeta-blaze" "https://github.com/sourcemeta/blaze"
git_clone_or_pull "cpp" "pboettch-json-schema-validator" "https://github.com/pboettch/json-schema-validator"
git_clone_or_pull "cpp" "tristanpenman-valijson" "https://github.com/tristanpenman/valijson"

# Zig
git_clone_or_pull "zig" "pascalPost-json-schema-validator" "https://github.com/pascalPost/json-schema-validator"
git_clone_or_pull "zig" "travisstaloch-json-schema-gen" "https://github.com/travisstaloch/json-schema-gen"

# C#
git_clone_or_pull "csharp" "RicoSuter-NJsonSchema" "https://github.com/RicoSuter/NJsonSchema"
git_clone_or_pull "csharp" "json-everything-json-everything" "https://github.com/json-everything/json-everything"
git_clone_or_pull "csharp" "corvus-dotnet-Corvus.JsonSchema" "https://github.com/corvus-dotnet/Corvus.JsonSchema"

# PHP
git_clone_or_pull "php" "jsonrainbow-json-schema" "https://github.com/jsonrainbow/json-schema"
git_clone_or_pull "php" "opis-json-schema" "https://github.com/opis/json-schema"
git_clone_or_pull "php" "wol-soft-php-json-schema-model-generator" "https://github.com/wol-soft/php-json-schema-model-generator"

# Kotlin
git_clone_or_pull "kotlin" "pwall567-json-kotlin-schema-codegen" "https://github.com/pwall567/json-kotlin-schema-codegen"
git_clone_or_pull "kotlin" "pwall567-json-kotlin-schema" "https://github.com/pwall567/json-kotlin-schema"

# Lua
git_clone_or_pull "lua" "fperrad-lua-schema" "https://framagit.org/fperrad/lua-schema.git"
git_clone_or_pull "lua" "api7-jsonschema" "https://github.com/api7/jsonschema"

# Ruby
git_clone_or_pull "ruby" "davishmcclurg-json_schemer" "https://github.com/davishmcclurg/json_schemer"
git_clone_or_pull "ruby" "voxpupuli-json-schema" "https://github.com/voxpupuli/json-schema"

# Dart
git_clone_or_pull "dart" "Workiva-json_schema" "https://github.com/Workiva/json_schema"
# Omitted: prompts for username/password auth (repo may be private or restricted).
# git_clone_or_pull "dart" "joaopedrosouza-json_schema_to_freezed" "https://github.com/joaopedrosouza/json_schema_to_freezed"

# Swift
git_clone_or_pull "swift" "kylef-JSONSchema.swift" "https://github.com/kylef/JSONSchema.swift"
git_clone_or_pull "swift" "ajevans99-swift-json-schema" "https://github.com/ajevans99/swift-json-schema"
