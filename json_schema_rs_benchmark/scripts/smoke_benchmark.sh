#!/usr/bin/env bash
#
# smoke_benchmark.sh — the benchmark's correctness check without timing (`make benchmark_smoke`,
# #112): every cell of our library and CLI (`jsonschemars`) is probed once, as the timed modes
# probe it before measuring, from a quick debug build of the working tree. It answers "does every
# cell still work?" in about a minute, where `make benchmark` answers "how fast?" in 4 to 16.
#
# What it checks, for `jsonschemars` only (BENCH_TOOLS restricts the harness to it):
#   - every in-process cell (codegen, schema_compile, both validations, and the generated-types
#     cells: every fixture's instances deserialized into our codegen's types, with and without
#     constrained newtypes), through the heap profilers' correctness checks;
#   - every CLI cell (codegen, validate_valid, validate_invalid) on every fixture, for every
#     valid and invalid instance: the command must exit 0 (codegen, valid) or not 0 (invalid);
#   - that every fixture's generated types compile (the isolated crate's `generated-types` build).
# A failing cell fails the run (exit 1), listing each one with its reason: every cell of our tool
# passes, so there is no baseline to compare against.
#
# Why a debug build: nothing is timed, and a debug build of the library and every fixture's
# generated types takes under a minute from cold (the benchmark profile's one codegen unit and
# thin LTO take about 6 min). It also runs the generated code with debug assertions and overflow
# checks, and with debug-sized stack frames, which found #120. Builds go to
# target/benchmark-smoke (its own CARGO_TARGET_DIR, kept between runs, so later builds are
# incremental), without debug info (CARGO_PROFILE_DEV_DEBUG=0: smaller and faster to link).
#
# Must stay compatible with bash 3.2 (the macOS system bash).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CRATE_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
REPO_ROOT="$(cd "${CRATE_DIR}/.." && pwd)"
COMMITTED_FIXTURES="${CRATE_DIR}/fixtures"
GENERATED_FIXTURES="${REPO_ROOT}/target/benchmark-fixtures"
SMOKE_DIR="${REPO_ROOT}/target/benchmark-smoke"
BIN_DIR="${SMOKE_DIR}/debug"
ISOLATED_TARGET="${SMOKE_DIR}/isolated"
OUT_DIR="${SMOKE_DIR}/out"
TOOL="jsonschemars"
export REPO_ROOT

# shellcheck source=tools.sh
source "${SCRIPT_DIR}/tools.sh"

# The CLI commands tools.sh builds name the smoke build's binary.
_jsonschemars_bin() { printf '%s/jsonschemars' "${BIN_DIR}"; }

# _cli_check <fixture id> <operation> <expect: zero|nonzero> <command line>: probe one CLI cell
# and record a failure in OUT_DIR/failures.tsv.
_cli_check() {
    local fid="$1" op="$2" expect="$3" cmd="$4" rc=0
    run_cmd_offline "${cmd}" > "${OUT_DIR}/cli.out" 2>&1 || rc=$?
    CLI_CHECKED=$(( CLI_CHECKED + 1 ))
    if [[ ( "${expect}" == zero && "${rc}" -ne 0 ) || ( "${expect}" == nonzero && "${rc}" -eq 0 ) ]]; then
        printf 'cli\t%s\t%s\texit %s, expected %s: %s\n' "${fid}" "${op}" "${rc}" "${expect}" \
            "$(tr '\n' ' ' < "${OUT_DIR}/cli.out" | cut -c1-300)" >> "${OUT_DIR}/failures.tsv"
    fi
}

# check_cli: every CLI cell of every fixture, every instance.
check_cli() {
    local fid fdir resource_count unresolved schema instance kind expect out
    local -a resources
    while IFS="$(printf '\t')" read -r fid fdir resource_count unresolved; do
        [[ -n "${fid}" && -z "${unresolved}" ]] || continue
        schema="${fdir}/schema.json"
        resources=()
        if [[ "${resource_count:-0}" -gt 0 ]]; then
            while IFS= read -r resource; do
                [[ -n "${resource}" ]] && resources+=("${resource}")
            done <<< "$(find "${fdir}/resources" -mindepth 1 -maxdepth 1 -name '*.json' | LC_ALL=C sort)"
        fi
        out="${OUT_DIR}/codegen/${fid//\//__}"
        mkdir -p "${out}"
        _cli_check "${fid}" codegen zero \
            "$(codegen_cmd "${TOOL}" "${schema}" "${out}" ${resources[@]+"${resources[@]}"})"
        for kind in valid invalid; do
            if [[ "${kind}" == valid ]]; then expect=zero; else expect=nonzero; fi
            while IFS= read -r instance; do
                [[ -n "${instance}" ]] || continue
                _cli_check "${fid}" "validate_${kind} ($(basename "${instance}"))" "${expect}" \
                    "$(validate_cmd "${TOOL}" "${schema}" "${instance}" ${resources[@]+"${resources[@]}"})"
            done <<< "$(find "${fdir}/instances/${kind}" -mindepth 1 -maxdepth 1 -name '*.json' 2>/dev/null | LC_ALL=C sort)"
        done
    done <<< "$("${BIN_DIR}/list_fixtures")"
}

main() {
    local failures in_process
    echo "==> smoke check: ${TOOL}, the working tree (debug build, no timing)"
    export CARGO_PROFILE_DEV_DEBUG=0
    echo "==> building"
    CARGO_TARGET_DIR="${SMOKE_DIR}" cargo build --manifest-path "${REPO_ROOT}/Cargo.toml" \
        -p json-schema-rs --bin jsonschemars \
        -p json-schema-rs-benchmark --bin heap_profile --bin gen_fixtures --bin list_fixtures
    "${BIN_DIR}/gen_fixtures" --output "${GENERATED_FIXTURES}"
    export BENCH_FIXTURE_ROOTS="${COMMITTED_FIXTURES}:${GENERATED_FIXTURES}"
    export BENCH_TOOLS="${TOOL}"
    BENCH_OFFLINE_WRAPPER="$(_offline_wrapper)"
    export BENCH_OFFLINE_WRAPPER
    CARGO_TARGET_DIR="${ISOLATED_TARGET}" cargo build \
        --manifest-path "${CRATE_DIR}/isolated/Cargo.toml" --features generated-types \
        --bin isolated_heap_profile

    rm -rf "${OUT_DIR}"
    mkdir -p "${OUT_DIR}"
    : > "${OUT_DIR}/failures.tsv"
    echo "==> in-process cells"
    "${BIN_DIR}/heap_profile" --output "${OUT_DIR}" 2> "${OUT_DIR}/heap.log" \
        || { echo "!! heap_profile failed (see ${OUT_DIR}/heap.log)" >&2; exit 1; }
    "${ISOLATED_TARGET}/debug/isolated_heap_profile" --output "${OUT_DIR}" \
        2>> "${OUT_DIR}/heap.log" \
        || { echo "!! isolated_heap_profile failed (see ${OUT_DIR}/heap.log)" >&2; exit 1; }
    in_process="$(wc -l < "${OUT_DIR}/inprocess_compat.tsv" | tr -d ' ')"
    awk -F '\t' '$4 != "pass" { printf "in-process\t%s\t%s\t%s\n", $2, $3, $5 }' \
        "${OUT_DIR}/inprocess_compat.tsv" >> "${OUT_DIR}/failures.tsv"

    echo "==> CLI cells"
    CLI_CHECKED=0
    check_cli

    failures="$(wc -l < "${OUT_DIR}/failures.tsv" | tr -d ' ')"
    if [[ "${failures}" -gt 0 ]]; then
        echo "!! ${failures} of $(( in_process + CLI_CHECKED )) cells failed:" >&2
        sed 's/^/   /' "${OUT_DIR}/failures.tsv" >&2
        exit 1
    fi
    echo "==> all $(( in_process + CLI_CHECKED )) cells pass (${in_process} in-process, ${CLI_CHECKED} CLI) in $(( SECONDS / 60 ))m$(printf '%02d' $(( SECONDS % 60 )))s"
}

# Run only when executed, so the functions above can be sourced and exercised on their own.
if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
    main "$@"
fi
