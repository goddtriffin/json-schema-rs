#!/usr/bin/env bash
#
# tools.sh — per-competitor CLI invocation definitions for the benchmark harness.
#
# OWNER: competitor-wiring leaf. This file is `source`d by scripts/run_benchmark.sh; it does
# not run standalone and intentionally has no `set -e` / top-level side effects. The driver
# exports `REPO_ROOT` (repo root) before sourcing us; our own release binaries live at
# "$REPO_ROOT/target/release/{jsonschemars,boon_validate}".
#
# ---------------------------------------------------------------------------------------
# CONTRACT (the driver depends on these EXACT function names / argument orders):
#
#   codegen_tools
#       -> echo space-separated ids of AVAILABLE codegen tools
#          (subset of: jsonschemars typify schemafy)
#   validation_tools
#       -> echo space-separated ids of AVAILABLE validation tools
#          (subset of: jsonschemars jsonschema-cli boon)
#   run_codegen  <tool> <schema_file> <out_dir>
#       -> generate Rust for <schema_file> into <out_dir>; return 0 ok / non-zero on failure
#   run_validate <tool> <schema_file> <payload_file>
#       -> validate <payload_file> against <schema_file>; return 0 valid / non-zero invalid
#   tool_display_name <id>
#       -> echo a human-friendly label for <id> (echoes the id itself if unknown)
#
# Availability detection MUST be silent and MUST NOT error: an uninstalled tool is simply
# omitted from codegen_tools / validation_tools (never a hard failure).
#
# ---------------------------------------------------------------------------------------
# COMPETITOR CLI SYNTAXES (researched 2026-07-03; sources inline):
#
#   typify  (cargo-typify, https://github.com/oxidecomputer/typify
#            + cargo-typify/README.md):
#       cargo typify <schema.json> --output <out.rs>
#     - Input schema is a POSITIONAL argument.
#     - `--output`/`-o` overrides the default output path (default = input path with the
#       extension replaced by `.rs`); `-` means stdout.
#     - Detected as the `cargo-typify` binary on PATH (cargo subcommand form: `cargo typify`).
#
#   schemafy (https://github.com/Marwes/schemafy):
#     - NO command-line interface. schemafy is a proc-macro / library crate only
#       (`schemafy!` macro + build-time codegen); there is no standalone binary to invoke.
#     - Therefore it CANNOT be wired as a CLI codegen tool here and is never reported by
#       codegen_tools. `run_codegen schemafy ...` returns non-zero (unsupported) so the
#       driver records it as a compat failure rather than crashing. Left in the id set for
#       forward-compat if a wrapper binary is ever added.
#
#   jsonschema-cli (https://github.com/Stranger6667/jsonschema, crate `jsonschema-cli`,
#                   binary name `jsonschema-cli`):
#       jsonschema-cli validate <schema.json> -i <instance.json>
#     - Schema is a POSITIONAL argument; instance via `-i`/`--instance` (repeatable).
#     - Exit 0 = all instances valid; exit 1 = invalid or error.
#     - Detected via `command -v jsonschema-cli`.
#
#   boon (our wrapper): "$REPO_ROOT/target/release/boon_validate" -s <schema> -p <payload>
#     - Always available once the benchmark crate is built (it is one of our own bins).
#
#   jsonschemars (our own CLI, "$REPO_ROOT/target/release/jsonschemars"):
#       codegen:  jsonschemars generate rust -o <out_dir> <schema>
#       validate: jsonschemars validate -s <schema> -p <payload>
#     - Always available once the crate is built (built from this repo).
# ---------------------------------------------------------------------------------------

# --- helpers ---------------------------------------------------------------------------

# _jsonschemars_bin / _boon_bin: absolute paths to our own release binaries.
_jsonschemars_bin() { printf '%s/target/release/jsonschemars' "${REPO_ROOT:?REPO_ROOT not set}"; }
_boon_bin() { printf '%s/target/release/boon_validate' "${REPO_ROOT:?REPO_ROOT not set}"; }

# _has <cmd>: true if <cmd> is on PATH (silent).
_has() { command -v "$1" >/dev/null 2>&1; }

# --- required-tool enforcement ---------------------------------------------------------

# required_external_tools: echo the external commands the harness REQUIRES. Every competitor
# we benchmark plus Hyperfine (wall-time) must be installed; a partial run would silently
# omit competitors and misrepresent the results. Our own bins (jsonschemars, boon_validate)
# are built by the driver and so are not listed here.
required_external_tools() {
    printf '%s\n' "hyperfine cargo-typify jsonschema-cli"
}

# _install_hint <cmd>: how to install a required tool.
_install_hint() {
    case "$1" in
        hyperfine)      echo "cargo install hyperfine" ;;
        cargo-typify)   echo "cargo install cargo-typify" ;;
        jsonschema-cli) echo "cargo install jsonschema-cli" ;;
        *)              echo "(unknown install method)" ;;
    esac
}

# check_required_tools: returns 0 if every required external tool is present; otherwise
# prints each missing tool with its install command and returns 1. The driver treats a
# non-zero return as a hard failure (no partial benchmarks).
check_required_tools() {
    local missing=0 tool
    for tool in $(required_external_tools); do
        if ! _has "${tool}"; then
            echo "  MISSING: ${tool}  (install: $(_install_hint "${tool}"))" >&2
            missing=1
        fi
    done
    return "${missing}"
}

# --- availability ----------------------------------------------------------------------

# codegen_tools: echo available codegen tool ids.
# jsonschemars is always available (our own bin). typify requires cargo-typify on PATH.
# schemafy has no CLI and is therefore never reported.
codegen_tools() {
    local tools="jsonschemars"
    if _has cargo-typify; then
        tools="${tools} typify"
    fi
    printf '%s\n' "${tools}"
}

# validation_tools: echo available validation tool ids.
# jsonschemars and boon are always available (our own bins). jsonschema-cli requires the
# `jsonschema` binary on PATH.
validation_tools() {
    local tools="jsonschemars boon"
    if _has jsonschema-cli; then
        tools="${tools} jsonschema-cli"
    fi
    printf '%s\n' "${tools}"
}

# --- codegen ---------------------------------------------------------------------------

# run_codegen <tool> <schema_file> <out_dir> -> 0 on success, non-zero on failure.
run_codegen() {
    local tool="$1" schema="$2" out_dir="$3"

    case "${tool}" in
        jsonschemars)
            mkdir -p "${out_dir}" || return 1
            "$(_jsonschemars_bin)" generate rust -o "${out_dir}" "${schema}"
            ;;
        typify)
            _has cargo-typify || return 1
            mkdir -p "${out_dir}" || return 1
            # typify emits a single .rs file; name it after the schema's basename.
            local base
            base="$(basename "${schema}")"
            base="${base%.*}"
            cargo typify "${schema}" --output "${out_dir}/${base}.rs"
            ;;
        schemafy)
            # No CLI available; report unsupported so the driver logs a compat failure.
            echo "run_codegen: schemafy has no CLI (proc-macro only); skipping" >&2
            return 1
            ;;
        *)
            echo "run_codegen: unknown codegen tool '${tool}'" >&2
            return 2
            ;;
    esac
}

# --- validation ------------------------------------------------------------------------

# run_validate <tool> <schema_file> <payload_file> -> 0 valid, non-zero invalid/error.
run_validate() {
    local tool="$1" schema="$2" payload="$3"

    case "${tool}" in
        jsonschemars)
            "$(_jsonschemars_bin)" validate -s "${schema}" -p "${payload}"
            ;;
        boon)
            "$(_boon_bin)" -s "${schema}" -p "${payload}"
            ;;
        jsonschema-cli)
            _has jsonschema-cli || return 1
            jsonschema-cli validate "${schema}" -i "${payload}"
            ;;
        *)
            echo "run_validate: unknown validation tool '${tool}'" >&2
            return 2
            ;;
    esac
}

# --- display names ---------------------------------------------------------------------

# tool_display_name <id> -> nice label (echoes id if unknown).
tool_display_name() {
    case "$1" in
        jsonschemars) echo "json-schema-rs" ;;
        typify) echo "typify (cargo typify)" ;;
        schemafy) echo "schemafy" ;;
        boon) echo "boon" ;;
        jsonschema-cli) echo "jsonschema-cli (Stranger6667)" ;;
        *) echo "$1" ;;
    esac
}
