#!/usr/bin/env bash
#
# quick_benchmark.sh — the quick benchmark every PR runs (`make benchmark`): our library and CLI
# only, on a base commit (default: where the branch left `main`) and on the working tree, built
# side by side and measured in the same session. It prints the PR body's *Performance* section and
# never touches the committed results (`make benchmark_full` refreshes those).
#
# Why the same session, cell by cell, from equal-length paths (#100; see design.md, *Quick
# mode*): the machine is shared (other builds, benchmarks, sessions), so its speed drifts over
# minutes and load arrives in bursts of seconds. Results from another day differ by up to 10%
# in-process, and whole-tier rounds a minute apart disagreed on 44 of 48 in-process cells in an
# A/A run (both sides the same commit). So every cell runs its rounds back to back, alternating
# the sides (base, head, head, base, base, head for three rounds), and `compare` summarizes each
# round by its lowest sample (the one no burst reached) and each side by the median of its
# rounds. Both sides' executables run from copies at paths of the same length: the path sits on
# the process's stack, and a longer one slowed an identical binary's sub-microsecond cells by up
# to 5.5%.
#
# What it measures, for `jsonschemars` only (BENCH_TOOLS restricts the harness to it):
#   - in-process peak heap (heap_profile, isolated_heap_profile), once per side: it is exact;
#   - in-process time (criterion), cell by cell (BENCH_CELL), BENCH_QUICK_ROUNDS rounds per side
#     in ABBA order: the main crate's cells (cross_tool) and the generated-types cells (isolated/'s
#     `generated-types` build: every fixture's instances deserialized into our codegen's types,
#     built from that side's codegen and library);
#   - CLI time and peak RSS (hyperfine), BENCH_QUICK_ROUNDS rounds, each running every cell
#     with its two sides back to back (base first in odd rounds), on every fixture: codegen,
#     validate_valid, validate_invalid, and the startup baseline.
# Reverse codegen is not measured here: its models take minutes to build per side (#102). A PR
# that changes the derive or reverse codegen runs `make benchmark_full` instead.
# A side whose isolated/ has no `generated-types` feature (a base older than it) has no
# generated-types cells, and compare lists the head's apart.
#
# Every cell is probed for correctness first (as in the full run); a cell measured on only one
# side is listed apart, since a cell that stops passing its check is a correctness change.
#
# Layout (QUICK_DIR = target/benchmark-quick):
#   base-src/       the base commit's tree (git archive; re-extracted when the base moves), with
#                   the working tree's Cargo.lock so both sides build the same dependencies
#   base-target/    its CARGO_TARGET_DIR (kept between runs, so later builds are incremental)
#   isolated-{base,head}/  each side's CARGO_TARGET_DIR for isolated/'s generated-types build (its
#                   own workspace). Both build with every source and target path remapped
#                   (--remap-path-prefix: the side's root to /src, this directory to /target):
#                   isolated/ reaches the library through path dependencies outside its
#                   workspace, which cargo compiles under their absolute paths, and the base's
#                   are longer (base-src/...), a compiled-in path difference (#100)
#   {base,head}-model-exclude.tsv  each side's model exclusions (see build_models in tools.sh)
#   run/{base,head}/ each side's timed executables (the criterion benches and jsonschemars), copied
#                   to paths of the same length: the executable's path sits on the process's
#                   stack, and a longer one shifted an identical binary's sub-microsecond cells by
#                   up to 5.5% (#100)
#   {base,head}/    each side's captures: heap/, criterion/<round>/, cli/, models/ (the model
#                   build's cargo messages), cells-{cross_tool,isolated}.txt (each bench's cells)
#   report.md       the comparison (also printed)
# The head side builds in the workspace's own target directory, like the full run.
#
# Env knobs:
#   BENCH_BASE            the base ref (default: main); the base commit is its merge base with
#                         HEAD, so changes that landed on it since the branch left are not
#                         counted. BENCH_BASE=HEAD with a clean tree is an A/A control.
#   BENCH_QUICK_ROUNDS    rounds per side (default 3)
#   BENCH_QUICK_CONFIRM_ROUNDS  further rounds per side for each cell that moved (default 4; 0
#                         skips confirmation)
#   BENCH_CRITERION_ARGS  criterion args (default: --warm-up-time 0.1 --measurement-time 0.3
#                         --sample-size 20 --noplot), per cell and round
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
RUN_DIR="${QUICK_DIR}/run"
TOOL="jsonschemars"
ROUNDS="${BENCH_QUICK_ROUNDS:-3}"
CONFIRM_ROUNDS="${BENCH_QUICK_CONFIRM_ROUNDS:-4}"
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
    # Both sides build against the same dependency versions (Cargo.lock is not committed). Each
    # lock file is copied only when it differs: a path package's fingerprint is the newest
    # modification time in its directory, so a fresh copy of isolated/Cargo.lock on every run
    # rebuilt the base's isolated crate (#112).
    local lock
    for lock in Cargo.lock json_schema_rs_benchmark/isolated/Cargo.lock; do
        if [[ -f "${REPO_ROOT}/${lock}" && -d "$(dirname "${BASE_SRC}/${lock}")" ]] \
            && ! cmp -s "${REPO_ROOT}/${lock}" "${BASE_SRC}/${lock}"; then
            cp "${REPO_ROOT}/${lock}" "${BASE_SRC}/${lock}"
        fi
    done
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

# list_cells <bench exe>: the in-process cells of our tool that the side's harness checks
# correct, one `<operation>/<fixture id>` per line (none when the executable does not exist).
list_cells() {
    [[ -x "$1" ]] || return 0
    "$1" --bench --list 2>/dev/null | sed -n "s|^\([a-z_]*\)/${TOOL}/\(.*\): benchmark\$|\1/\2|p"
}

# record_cells <side>: list each of the side's benches' cells into its cells-<bench>.txt.
record_cells() {
    local side="$1" bench
    for bench in cross_tool isolated; do
        list_cells "${RUN_DIR}/${side}/${bench}" > "${QUICK_DIR}/${side}/cells-${bench}.txt"
    done
}

# cell_bench <side> <operation/fixture id>: the side's bench that measures the cell (cross_tool or
# isolated); nothing when neither does (its check fails on that side).
cell_bench() {
    local side="$1" cell="$2" bench
    for bench in cross_tool isolated; do
        if grep -qxF "${cell}" "${QUICK_DIR}/${side}/cells-${bench}.txt"; then
            printf '%s\n' "${bench}"
            return 0
        fi
    done
}

# run_criterion_cell <side> <bench exe> <round> <operation/fixture id>: one criterion round of
# one cell. BENCH_CELL restricts the harness to it (the base side's harness may predate it, so
# the exact criterion filter restricts the timing too).
run_criterion_cell() {
    local side="$1" exe="$2" round="$3" cell="$4" filter
    local -a criterion_args
    # Word-split the knob on purpose: it is a list of criterion flags.
    # shellcheck disable=SC2206
    criterion_args=(${BENCH_CRITERION_ARGS:---warm-up-time 0.1 --measurement-time 0.3 --sample-size 20 --noplot})
    filter="${cell%%/*}/${TOOL}/${cell#*/}"
    filter="^$(printf '%s' "${filter}" | sed 's/[][\.*^$+?(){}|]/\\&/g')\$"
    BENCH_CELL="${cell}" CRITERION_HOME="${QUICK_DIR}/${side}/criterion/${round}" "${exe}" \
        --bench "${filter}" ${criterion_args[@]+"${criterion_args[@]}"} \
        >> "${QUICK_DIR}/${side}/criterion.log" 2>&1 \
        || { echo "!! criterion failed on ${cell} (see ${QUICK_DIR}/${side}/criterion.log)" >&2; exit 1; }
}

# run_inprocess <first round> <last round> [cells file]: every cell's criterion rounds back to
# back, the sides alternating per round (base first in odd rounds), each side on the bench that
# lists the cell (record_cells); only the cells the file lists, when given. A cell one side does
# not list (its check fails there) is measured on the other side only, and compare lists it apart.
run_inprocess() {
    local first="$1" last="$2" only="${3:-}"
    local cells cell round side bench count
    local -a sides
    cells="$(cat "${QUICK_DIR}"/base/cells-*.txt "${QUICK_DIR}"/head/cells-*.txt \
        | sed '/^$/d' | LC_ALL=C sort -u)"
    if [[ -n "${only}" ]]; then
        cells="$(printf '%s\n' "${cells}" | grep -xF -f "${only}" || true)"
    fi
    count="$(printf '%s\n' "${cells}" | sed '/^$/d' | wc -l | tr -d ' ')"
    echo "==> in-process time: ${count} cells, rounds ${first}-${last} per side, interleaved per cell"
    while IFS= read -r cell; do
        [[ -n "${cell}" ]] || continue
        for round in $(seq "${first}" "${last}"); do
            if (( round % 2 == 1 )); then sides=(base head); else sides=(head base); fi
            for side in "${sides[@]}"; do
                bench="$(cell_bench "${side}" "${cell}")"
                if [[ -n "${bench}" ]]; then
                    run_criterion_cell "${side}" "${RUN_DIR}/${side}/${bench}" "${round}" "${cell}"
                fi
            done
        done
    done <<< "${cells}"
}

# build_generated_types <side> <source root>: build the side's isolated/ `generated-types` bench
# and heap profiler (see isolated-{base,head}/ in the layout above) and print the bench
# executable's path; print nothing when the side's isolated/ has no such feature. Needs the
# fixture roots (the model shards generate their types from them at build time).
build_generated_types() {
    local side="$1" root="$2" manifest target exclude exe
    local -a remap
    manifest="${root}/json_schema_rs_benchmark/isolated/Cargo.toml"
    if ! grep -q '^generated-types = ' "${manifest}"; then
        echo "    ${side}: isolated/ has no generated-types feature; no generated-types cells" >&2
        return 0
    fi
    target="${QUICK_DIR}/isolated-${side}"
    exclude="${QUICK_DIR}/${side}-model-exclude.tsv"
    clear_exclusions "${exclude}"
    remap=(--config "build.rustflags = [\"--remap-path-prefix=${root}=/src\", \"--remap-path-prefix=${target}=/target\"]")
    CARGO_TARGET_DIR="${target}" build_models "${manifest}" generated-types "${exclude}" \
        "${QUICK_DIR}/${side}/models" "${remap[@]}" >&2 || exit 1
    exe="$(CARGO_TARGET_DIR="${target}" cargo bench --manifest-path "${manifest}" \
        --config "${PROFILE_CONFIG}" --profile benchmark --features generated-types "${remap[@]}" \
        --bench isolated --no-run --message-format=json \
        | grep -o '"executable":"[^"]*"' | grep '/deps/isolated-' | tail -n 1 | cut -d '"' -f 4)"
    if [[ -z "${exe}" || ! -x "${exe}" ]]; then
        echo "!! no isolated bench executable built under ${target}" >&2
        exit 1
    fi
    printf '%s\n' "${exe}"
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

# run_cli <round> [cells file]: one round of every CLI cell (only the `<operation>/<fixture id>`
# cells the file lists, when given), both sides, in the round's order. A cell whose probe fails
# on a side is not timed there.
run_cli() {
    local round="$1" only="${2:-}" fixtures fid fdir resource_count unresolved schema valid invalid out
    local build_bin op expect payload cmd side
    local -a resources sides
    build_bin="$(_jsonschemars_bin)"
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
            if [[ -n "${only}" ]] && ! grep -qxF "${op}/${fid}" "${only}"; then
                continue
            fi
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
                local side_cmd="${cmd//"${build_bin}"/"${RUN_DIR}/${side}/jsonschemars"}"
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

# _minutes <seconds>: the duration as minutes and seconds.
_minutes() {
    printf '%dm%02ds' $(( $1 / 60 )) $(( $1 % 60 ))
}

main() {
    local base_ref base_sha base_exe head_exe base_types_exe head_types_exe round side
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
        --bin gen_fixtures --bin list_fixtures --bin compare --bin model_build_failures
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

    echo "==> building the generated types: the working tree"
    head_types_exe="$(build_generated_types head "${REPO_ROOT}")"
    echo "==> building the generated types: ${base_ref} (${base_sha:0:9})"
    base_types_exe="$(build_generated_types base "${BASE_SRC}")"

    local built_at="${SECONDS}"

    # Time both sides from paths of the same length (see RUN_DIR in the layout above).
    rm -rf "${RUN_DIR}"
    mkdir -p "${RUN_DIR}/base" "${RUN_DIR}/head"
    cp "${base_exe}" "${RUN_DIR}/base/cross_tool"
    cp "${BASE_BUILD_DIR}/jsonschemars" "${RUN_DIR}/base/jsonschemars"
    cp "${head_exe}" "${RUN_DIR}/head/cross_tool"
    cp "$(_jsonschemars_bin)" "${RUN_DIR}/head/jsonschemars"
    if [[ -n "${base_types_exe}" ]]; then
        cp "${base_types_exe}" "${RUN_DIR}/base/isolated"
    fi
    if [[ -n "${head_types_exe}" ]]; then
        cp "${head_types_exe}" "${RUN_DIR}/head/isolated"
    fi

    echo "==> in-process peak heap"
    "${BASE_BUILD_DIR}/heap_profile" --output "${QUICK_DIR}/base/heap" 2> "${QUICK_DIR}/base/heap.log" \
        || { echo "!! base heap profile failed (see ${QUICK_DIR}/base/heap.log)" >&2; exit 1; }
    "${BUILD_DIR}/heap_profile" --output "${QUICK_DIR}/head/heap" 2> "${QUICK_DIR}/head/heap.log" \
        || { echo "!! heap profile failed (see ${QUICK_DIR}/head/heap.log)" >&2; exit 1; }
    for side in base head; do
        local profiler="${QUICK_DIR}/isolated-${side}/benchmark/isolated_heap_profile"
        if [[ -x "${RUN_DIR}/${side}/isolated" ]]; then
            "${profiler}" --output "${QUICK_DIR}/${side}/heap" 2>> "${QUICK_DIR}/${side}/heap.log" \
                || { echo "!! ${side} generated-types heap profile failed (see ${QUICK_DIR}/${side}/heap.log)" >&2; exit 1; }
        fi
        record_cells "${side}"
    done

    run_inprocess 1 "${ROUNDS}"
    for round in $(seq 1 "${ROUNDS}"); do
        run_cli "${round}"
    done

    # Confirmation: re-measure every cell that moved in CONFIRM_ROUNDS further rounds, so the
    # final comparison of those cells rests on more processes per side (see design.md).
    if (( CONFIRM_ROUNDS > 0 )); then
        "${BUILD_DIR}/compare" --base "${QUICK_DIR}/base" --head "${QUICK_DIR}/head" \
            --tool "${TOOL}" --base-label "${base_ref}" \
            --moved-in-process "${QUICK_DIR}/moved-in-process.txt" \
            --moved-cli "${QUICK_DIR}/moved-cli.txt" > /dev/null
        local first=$(( ROUNDS + 1 )) last=$(( ROUNDS + CONFIRM_ROUNDS ))
        if [[ -s "${QUICK_DIR}/moved-in-process.txt" ]]; then
            echo "==> confirming the moved in-process cells"
            run_inprocess "${first}" "${last}" "${QUICK_DIR}/moved-in-process.txt"
        fi
        if [[ -s "${QUICK_DIR}/moved-cli.txt" ]]; then
            echo "==> confirming the moved CLI cells"
            for round in $(seq "${first}" "${last}"); do
                run_cli "${round}" "${QUICK_DIR}/moved-cli.txt"
            done
        fi
    fi
    rm -rf "${QUICK_DIR}/work"

    echo "==> report (${QUICK_DIR}/report.md); builds $(_minutes "${built_at}"), measurement $(_minutes $(( SECONDS - built_at )))"
    echo
    "${BUILD_DIR}/compare" --base "${QUICK_DIR}/base" --head "${QUICK_DIR}/head" \
        --tool "${TOOL}" --base-label "${base_ref}" --output "${QUICK_DIR}/report.md"
}

# Run only when executed, so the functions above can be sourced and exercised on their own.
if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
    main "$@"
fi
