#!/usr/bin/env bash
#
# quick_benchmark.sh — the quick benchmark every PR runs (`make benchmark`): our library and CLI
# only, on a base commit (default: where the branch left `main`) and on the working tree, built
# side by side and measured in the same session. It prints the PR body's *Performance* section and
# never touches the committed results (`make benchmark_full` refreshes those).
#
# Why the same session: cells drift by up to 10% in-process, and much more on the CLI tier,
# between runs on different days (#100). Measuring both sides now, in interleaved rounds whose
# samples are pooled per side, lets that drift land on both alike, so only what the change moved
# shows up.
#
# What it measures, for `jsonschemars` only (BENCH_TOOLS restricts the harness to it):
#   - in-process peak heap (heap_profile), once per side: it is exact;
#   - in-process time (criterion), BENCH_QUICK_ROUNDS rounds per side in ABBA order;
#   - CLI time and peak RSS (hyperfine), the same rounds, on every fixture: codegen,
#     validate_valid, validate_invalid, and the startup baseline.
# Reverse codegen is not measured here: its models take minutes to build per side (#102). A PR
# that changes the derive or reverse codegen runs `make benchmark_full` instead.
#
# Every cell is probed for correctness first (as in the full run); a cell measured on only one
# side is listed apart, since a cell that stops passing its check is a correctness change.
#
# Layout (QUICK_DIR = target/benchmark-quick):
#   base-src/       the base commit's tree (git archive; re-extracted when the base moves), with
#                   the working tree's Cargo.lock so both sides build the same dependencies
#   base-target/    its CARGO_TARGET_DIR (kept between runs, so later builds are incremental)
#   {base,head}/    each side's captures: heap/, criterion/<round>/, cli/
#   report.md       the comparison (also printed)
# The head side builds in the workspace's own target directory, like the full run.
#
# Env knobs:
#   BENCH_BASE            the base ref (default: main); the base commit is its merge base with
#                         HEAD, so changes that landed on it since the branch left are not
#                         counted. BENCH_BASE=HEAD with a clean tree is an A/A control.
#   BENCH_QUICK_ROUNDS    rounds per side (default 2)
#   BENCH_CRITERION_ARGS  criterion args (default: --warm-up-time 0.3 --measurement-time 1
#                         --sample-size 20)
#   BENCH_CLI_WARMUP      hyperfine warmup runs per cell and round (default 2)
#   BENCH_CLI_RUNS        hyperfine timed runs per cell and round (default 10)
#
# Must stay compatible with bash 3.2 (the macOS system bash).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CRATE_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
REPO_ROOT="$(cd "${CRATE_DIR}/.." && pwd)"
COMMITTED_FIXTURES="${CRATE_DIR}/fixtures"
GENERATED_FIXTURES="${REPO_ROOT}/target/benchmark-fixtures"
PROFILE_CONFIG="${CRATE_DIR}/profile.toml"
BUILD_DIR="${REPO_ROOT}/target/benchmark"
QUICK_DIR="${REPO_ROOT}/target/benchmark-quick"
BASE_SRC="${QUICK_DIR}/base-src"
BASE_TARGET="${QUICK_DIR}/base-target"
BASE_BUILD_DIR="${BASE_TARGET}/benchmark"
TOOL="jsonschemars"
ROUNDS="${BENCH_QUICK_ROUNDS:-2}"
export REPO_ROOT

# shellcheck source=tools.sh
source "${SCRIPT_DIR}/tools.sh"

# _profile_cargo <source root> <target dir> <subcommand> [cargo args...]: run a cargo subcommand on
# a side's workspace with the benchmark profile (the working tree's profile.toml for both sides).
_profile_cargo() {
    local root="$1" target="$2" subcommand="$3"
    shift 3
    CARGO_TARGET_DIR="${target}" cargo "${subcommand}" --manifest-path "${root}/Cargo.toml" \
        --config "${PROFILE_CONFIG}" --profile benchmark "$@"
}

# extract_base <sha>: put the base commit's tree in BASE_SRC, unless it is already there.
extract_base() {
    local sha="$1"
    if [[ "$(cat "${BASE_SRC}/.quick-base-sha" 2>/dev/null || true)" != "${sha}" ]]; then
        rm -rf "${BASE_SRC}"
        mkdir -p "${BASE_SRC}"
        git -C "${REPO_ROOT}" archive "${sha}" | tar -x -C "${BASE_SRC}"
        printf '%s\n' "${sha}" > "${BASE_SRC}/.quick-base-sha"
    fi
    # Both sides build against the same dependency versions (Cargo.lock is not committed).
    if [[ -f "${REPO_ROOT}/Cargo.lock" ]]; then
        cp "${REPO_ROOT}/Cargo.lock" "${BASE_SRC}/Cargo.lock"
    fi
}

# build_side <source root> <target dir>: build the side's CLI, heap profiler and criterion bench,
# and print the bench executable's path.
build_side() {
    local root="$1" target="$2" exe
    _profile_cargo "${root}" "${target}" build -p json-schema-rs --bin jsonschemars >&2
    _profile_cargo "${root}" "${target}" build -p json-schema-rs-benchmark --bin heap_profile >&2
    exe="$(_profile_cargo "${root}" "${target}" bench -p json-schema-rs-benchmark \
        --bench cross_tool --no-run --message-format=json \
        | grep -o '"executable":"[^"]*"' | grep cross_tool | tail -n 1 | cut -d '"' -f 4)"
    if [[ -z "${exe}" || ! -x "${exe}" ]]; then
        echo "!! no cross_tool bench executable built under ${target}" >&2
        exit 1
    fi
    printf '%s\n' "${exe}"
}

# run_criterion <side> <bench exe> <round>: one criterion round of our cells.
run_criterion() {
    local side="$1" exe="$2" round="$3"
    local -a criterion_args
    # Word-split the knob on purpose: it is a list of criterion flags.
    # shellcheck disable=SC2206
    criterion_args=(${BENCH_CRITERION_ARGS:---warm-up-time 0.3 --measurement-time 1 --sample-size 20})
    echo "==> in-process time: ${side}, round ${round}"
    CRITERION_HOME="${QUICK_DIR}/${side}/criterion/${round}" "${exe}" --bench "/${TOOL}/" \
        ${criterion_args[@]+"${criterion_args[@]}"} > "${QUICK_DIR}/${side}/criterion-${round}.log" 2>&1 \
        || { echo "!! criterion failed (see ${QUICK_DIR}/${side}/criterion-${round}.log)" >&2; exit 1; }
}

# _cli_probe <command line> <expect: zero|nonzero>: true when the command, run offline, exits as
# expected.
_cli_probe() {
    local rc=0
    run_cmd_offline "$1" >/dev/null 2>&1 || rc=$?
    case "$2" in
        zero)    [[ "${rc}" -eq 0 ]] ;;
        nonzero) [[ "${rc}" -ne 0 ]] ;;
    esac
}

# _cli_time <side> <fixture id> <operation> <expect> <command line> <round>: time one probed cell
# and index the export.
_cli_time() {
    local side="$1" fid="$2" op="$3" expect="$4" cmd="$5" round="$6"
    local rel json
    local -a hf_args
    rel="cli/${fid//\//__}__${op}__${round}.json"
    json="${QUICK_DIR}/${side}/${rel}"
    hf_args=(-N --warmup "${BENCH_CLI_WARMUP:-2}" --runs "${BENCH_CLI_RUNS:-10}" --style none
        --export-json "${json}")
    if [[ "${expect}" == "nonzero" ]]; then
        hf_args+=(-i)
    fi
    if hyperfine "${hf_args[@]}" "${cmd}" >/dev/null 2>&1; then
        printf '%s\t%s\t%s\t%s\n' "${TOOL}" "${fid}" "${op}" "${rel}" \
            >> "${QUICK_DIR}/${side}/cli/index.tsv"
    else
        echo "    ${side} ${fid} ${op}: hyperfine failed; not measured" >&2
    fi
}

# run_cli <round>: one round of every CLI cell, both sides, in the round's order. A cell whose
# probe fails on a side is not timed there.
run_cli() {
    local round="$1" fixtures fid fdir resource_count unresolved schema valid invalid out
    local head_bin base_bin op expect payload cmd side
    local -a resources sides
    head_bin="$(_jsonschemars_bin)"
    base_bin="${BASE_BUILD_DIR}/jsonschemars"
    if (( round % 2 == 1 )); then sides=(base head); else sides=(head base); fi
    echo "==> CLI: round ${round} (${sides[*]})"
    fixtures="$(printf 'startup\t%s\t0\t\n' "$(_write_startup "${QUICK_DIR}/work")")
$("${BUILD_DIR}/list_fixtures")"
    while IFS="$(printf '\t')" read -r fid fdir resource_count unresolved; do
        [[ -n "${fid}" && -z "${unresolved}" ]] || continue
        schema="${fdir}/schema.json"
        valid="$(find "${fdir}/instances/valid" -mindepth 1 -maxdepth 1 -name '*.json' 2>/dev/null | LC_ALL=C sort | head -n 1)"
        invalid="$(find "${fdir}/instances/invalid" -mindepth 1 -maxdepth 1 -name '*.json' 2>/dev/null | LC_ALL=C sort | head -n 1)"
        resources=()
        if [[ "${resource_count:-0}" -gt 0 ]]; then
            while IFS= read -r resource; do
                [[ -n "${resource}" ]] && resources+=("${resource}")
            done <<< "$(find "${fdir}/resources" -mindepth 1 -maxdepth 1 -name '*.json' | LC_ALL=C sort)"
        fi
        for op in codegen validate_valid validate_invalid; do
            case "${op}" in
                codegen)
                    expect=zero
                    out="${QUICK_DIR}/work/out/${fid//\//__}"
                    mkdir -p "${out}"
                    cmd="$(codegen_cmd "${TOOL}" "${schema}" "${out}" ${resources[@]+"${resources[@]}"})" ;;
                validate_valid | validate_invalid)
                    if [[ "${op}" == validate_valid ]]; then expect=zero; payload="${valid}"
                    else expect=nonzero; payload="${invalid}"; fi
                    [[ -n "${payload}" ]] || continue
                    cmd="$(validate_cmd "${TOOL}" "${schema}" "${payload}" ${resources[@]+"${resources[@]}"})" ;;
            esac
            for side in "${sides[@]}"; do
                local side_cmd="${cmd}"
                if [[ "${side}" == base ]]; then
                    side_cmd="${cmd//"${head_bin}"/"${base_bin}"}"
                fi
                if _cli_probe "${side_cmd}" "${expect}"; then
                    _cli_time "${side}" "${fid}" "${op}" "${expect}" "${side_cmd}" "${round}"
                elif [[ "${round}" == 1 ]]; then
                    echo "    ${side} ${fid} ${op}: probe failed; not measured"
                fi
            done
        done
    done <<< "${fixtures}"
}

# _write_startup <work dir>: write the startup baseline fixture (the full run's, see
# run_benchmark.sh) and print its directory.
_write_startup() {
    local dir="$1/startup"
    mkdir -p "${dir}/instances/valid" "${dir}/instances/invalid"
    printf '%s\n' '{"$schema": "http://json-schema.org/draft-07/schema#", "type": "object", "properties": {"name": {"type": "string"}}}' \
        > "${dir}/schema.json"
    printf '{}\n' > "${dir}/instances/valid/object.json"
    printf '[]\n' > "${dir}/instances/invalid/array.json"
    printf '%s\n' "${dir}"
}

main() {
    local base_ref base_sha base_exe head_exe round side
    base_ref="${BENCH_BASE:-main}"
    base_sha="$(git -C "${REPO_ROOT}" merge-base "${base_ref}" HEAD)"
    echo "==> quick benchmark: ${TOOL}, ${base_ref} (${base_sha:0:9}) vs the working tree"
    if ! _has hyperfine; then
        echo "!! hyperfine is missing (install: $(_install_hint hyperfine))" >&2
        exit 1
    fi

    echo "==> building the working tree ($(profile_summary))"
    head_exe="$(build_side "${REPO_ROOT}" "${REPO_ROOT}/target")"
    _profile_cargo "${REPO_ROOT}" "${REPO_ROOT}/target" build -p json-schema-rs-benchmark \
        --bin gen_fixtures --bin list_fixtures --bin compare
    echo "==> building ${base_ref} (${base_sha:0:9})"
    extract_base "${base_sha}"
    base_exe="$(build_side "${BASE_SRC}" "${BASE_TARGET}")"

    echo "==> generating synthetic fixtures"
    "${BUILD_DIR}/gen_fixtures" --output "${GENERATED_FIXTURES}"
    export BENCH_FIXTURE_ROOTS="${COMMITTED_FIXTURES}:${GENERATED_FIXTURES}"
    export BENCH_TOOLS="${TOOL}"
    BENCH_OFFLINE_WRAPPER="$(_offline_wrapper)"
    export BENCH_OFFLINE_WRAPPER

    rm -rf "${QUICK_DIR}/base" "${QUICK_DIR}/head" "${QUICK_DIR}/work"
    for side in base head; do
        mkdir -p "${QUICK_DIR}/${side}/cli" "${QUICK_DIR}/${side}/criterion"
        : > "${QUICK_DIR}/${side}/cli/index.tsv"
    done

    echo "==> in-process peak heap"
    "${BASE_BUILD_DIR}/heap_profile" --output "${QUICK_DIR}/base/heap" 2> "${QUICK_DIR}/base/heap.log" \
        || { echo "!! base heap profile failed (see ${QUICK_DIR}/base/heap.log)" >&2; exit 1; }
    "${BUILD_DIR}/heap_profile" --output "${QUICK_DIR}/head/heap" 2> "${QUICK_DIR}/head/heap.log" \
        || { echo "!! heap profile failed (see ${QUICK_DIR}/head/heap.log)" >&2; exit 1; }

    for round in $(seq 1 "${ROUNDS}"); do
        if (( round % 2 == 1 )); then
            run_criterion base "${base_exe}" "${round}"
            run_criterion head "${head_exe}" "${round}"
        else
            run_criterion head "${head_exe}" "${round}"
            run_criterion base "${base_exe}" "${round}"
        fi
    done
    for round in $(seq 1 "${ROUNDS}"); do
        run_cli "${round}"
    done
    rm -rf "${QUICK_DIR}/work"

    echo "==> report (${QUICK_DIR}/report.md)"
    echo
    "${BUILD_DIR}/compare" --base "${QUICK_DIR}/base" --head "${QUICK_DIR}/head" \
        --tool "${TOOL}" --base-label "${base_ref}" --output "${QUICK_DIR}/report.md"
}

# Run only when executed, so the functions above can be sourced and exercised on their own.
if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
    main "$@"
fi
