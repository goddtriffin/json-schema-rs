#!/usr/bin/env bash
#
# run_benchmark.sh — top-level driver for the json-schema-rs benchmark harness (`make benchmark`).
#
# The harness measures every tool in two tiers:
#
#   In-process tier — each tool linked as a Rust library and called on in-memory inputs, so
#     process startup and file I/O are excluded. Time comes from criterion (benches/cross_tool.rs;
#     median + spread); peak heap per operation comes from a counting allocator
#     (src/bin/heap_profile.rs). Operations: codegen, schema_compile, validate_valid,
#     validate_invalid. Tools: jsonschemars (our lib), jsonschema, boon, typify.
#
#   CLI tier — each tool's command-line front end timed end to end (startup, file I/O, parse,
#     compile, generate/validate, exit) by hyperfine, with warmup and repeated runs; hyperfine
#     also records each run's peak RSS. Operations: codegen, validate_valid, validate_invalid.
#     Tools and their invocations are defined in scripts/tools.sh.
#
# A tool is only measured on a fixture/operation it handles CORRECTLY (codegen succeeds; valid
# instance accepted; invalid instance rejected). Anything else is recorded as a compat failure and
# left untimed, so a fast failure never masquerades as a fast result. The run is hermetic: a
# fixture whose schema has an external $ref is not measured, and each CLI correctness probe runs
# with network access denied (macOS sandbox-exec / Linux unshare -rn), so a tool that downloads a
# remote document fails its probe instead of having the download timed. A tool missing from the
# machine is different: the driver aborts up front rather than silently omit a competitor.
#
# Pipeline:
#   1. Check required external tools, and that every competitor is its newest stable release
#      (linked crate == crates.io's newest stable; CLI == that crate's release), so a run never
#      measures against an outdated target. Record the versions.
#   2. Build release binaries (jsonschemars CLI + the benchmark crate's bins).
#   3. Generate the synthetic fixtures (target/benchmark-fixtures; deterministic, not committed).
#   4. Reset the raw capture dir and criterion's output dir.
#   5. In-process peak heap + compat matrix (heap_profile).
#   6. In-process timing (criterion).
#   7. CLI timing (hyperfine) + CLI compat matrix.
#   8. Aggregate everything into the committed results.json + results.md.
#
# Raw captures (RAW_DIR = json_schema_rs_benchmark/target/benchmark-raw):
#   tool_versions.tsv                 tool \t version
#   inprocess_heap.tsv, inprocess_compat.tsv   (written by heap_profile)
#   cli/compat.tsv                    tool \t fixture \t operation \t pass|fail \t detail
#   cli/index.tsv                     tool \t fixture \t operation \t cli/<hyperfine export>.json
#   cli/<tool>__<group>__<name>__<operation>.json   hyperfine --export-json, one result each
#
# Env knobs:
#   BENCH_CRITERION_ARGS    args passed to criterion (default: --warm-up-time 0.5
#                           --measurement-time 2 --sample-size 20)
#   BENCH_SKIP_CRITERION=1  skip the in-process timing step (faster iteration)
#   BENCH_CLI_WARMUP        hyperfine warmup runs per CLI measurement (default 3)
#   BENCH_CLI_RUNS          hyperfine timed runs per CLI measurement (default 15)
#
# Must stay compatible with bash 3.2 (the macOS system bash).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CRATE_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
REPO_ROOT="$(cd "${CRATE_DIR}/.." && pwd)"
COMMITTED_FIXTURES="${CRATE_DIR}/fixtures"
GENERATED_FIXTURES="${REPO_ROOT}/target/benchmark-fixtures"
RAW_DIR="${CRATE_DIR}/target/benchmark-raw"
CRITERION_DIR="${REPO_ROOT}/target/criterion"
RELEASE_DIR="${REPO_ROOT}/target/release"
export REPO_ROOT

# shellcheck source=tools.sh
source "${SCRIPT_DIR}/tools.sh"

# --- CLI tier ----------------------------------------------------------------------------

# _tsv_field <text>: collapse tabs/newlines to spaces and cap the length, for a TSV detail cell.
_tsv_field() {
    printf '%s' "$1" | tr '\t\r\n' '   ' | cut -c1-160
}

# _failure_summary: read a failed command's output on stdin and print its most informative line:
# a panic `Message:`, else the deepest cause of an error chain (`  N: cause`), else the first line
# mentioning an error or panic, else the first non-blank line. ANSI colour codes are stripped.
_failure_summary() {
    sed "s/$(printf '\033')\\[[0-9;]*m//g" | awk '
        /^[[:space:]]*$/ { next }
        first == "" { first = $0 }
        /^Message:/ && msg == "" { msg = $0; sub(/^Message:[[:space:]]*/, "", msg) }
        /^[[:space:]]*[0-9]+: / { cause = $0; sub(/^[[:space:]]*[0-9]+: /, "", cause) }
        /[Ee]rror|panicked/ && err == "" { err = $0 }
        END {
            if (msg != "") print msg
            else if (cause != "") print cause
            else if (err != "") print err
            else print first
        }'
}

# _first_instance <dir>: the first .json file in <dir>, sorted by file name ("" if none).
_first_instance() {
    [[ -d "$1" ]] || return 0
    find "$1" -mindepth 1 -maxdepth 1 -type f -name '*.json' | LC_ALL=C sort | head -n 1
}

# discover_fixtures: print "<fixture id>\t<fixture dir>\t<external refs>" for every fixture under
# BENCH_FIXTURE_ROOTS, sorted by id, using the in-process tier's discovery (list_fixtures) so both
# tiers measure the same set. <external refs> is empty for a self-contained schema.
discover_fixtures() {
    "${RELEASE_DIR}/list_fixtures"
}

# _cli_offline_skip <raw_dir> <tool> <fixture id> <operation> <external refs>: record a cell left
# unmeasured because the schema needs external documents. The benchmark runs offline, so timing it
# would measure a failed lookup or a network fetch rather than the operation. The detail text
# matches the in-process tier's (adapters::OFFLINE_SKIP).
_cli_offline_skip() {
    printf '%s\t%s\t%s\tfail\t%s\n' "$2" "$3" "$4" \
        "not measured: schema needs external documents (offline run)" >> "$1/cli/compat.tsv"
    echo "    $2 $4: skipped (schema needs external documents: ${5//,/, })"
}

# _cli_measure <raw_dir> <tool> <fixture id> <operation> <expect: zero|nonzero> <command line>
# Probe the command once for correctness, offline; on a correct result time it with hyperfine and index the
# export, otherwise record a compat failure and skip timing.
_cli_measure() {
    local raw_dir="$1" tool="$2" fid="$3" op="$4" expect="$5" cmd="$6"
    local rc=0 err ok reason rel json
    local -a hf_args

    # The probe runs offline: the benchmark is hermetic, so a tool that needs the network (to
    # fetch a remote $ref or meta-schema) fails here instead of having a download timed.
    err="$(run_cmd_offline "${cmd}" 2>&1)" || rc=$?
    ok=0
    case "${expect}" in
        zero)    [[ "${rc}" -eq 0 ]] && ok=1 ;;
        nonzero) [[ "${rc}" -ne 0 ]] && ok=1 ;;
    esac

    if [[ "${ok}" -eq 0 ]]; then
        case "${op}" in
            codegen)          reason="exit ${rc}" ;;
            validate_valid)   reason="rejected valid instance (exit ${rc})" ;;
            validate_invalid) reason="accepted invalid instance (exit 0)" ;;
            *)                reason="exit ${rc}" ;;
        esac
        if [[ -n "${err}" && "${expect}" == "zero" ]]; then
            reason="${reason}: $(printf '%s\n' "${err}" | _failure_summary)"
        fi
        printf '%s\t%s\t%s\tfail\t%s\n' "${tool}" "${fid}" "${op}" "$(_tsv_field "${reason}")" \
            >> "${raw_dir}/cli/compat.tsv"
        echo "    ${tool} ${op}: FAIL ($(_tsv_field "${reason}"))"
        return 0
    fi
    rel="cli/${tool}__${fid//\//__}__${op}.json"
    json="${raw_dir}/${rel}"
    hf_args=(-N --warmup "${BENCH_CLI_WARMUP:-3}" --runs "${BENCH_CLI_RUNS:-15}" --style none
        --export-json "${json}")
    if [[ "${expect}" == "nonzero" ]]; then
        hf_args+=(-i)
    fi
    # A probe-correct tool can still fail a later run (e.g. a crash): record it, never abort.
    if ! hyperfine "${hf_args[@]}" "${cmd}" >/dev/null 2>"${raw_dir}/cli/.hyperfine.err"; then
        reason="hyperfine: $(grep -m 1 -v '^[[:space:]]*$' "${raw_dir}/cli/.hyperfine.err" || true)"
        rm -f "${json}" "${raw_dir}/cli/.hyperfine.err"
        printf '%s\t%s\t%s\tfail\t%s\n' "${tool}" "${fid}" "${op}" "$(_tsv_field "${reason}")" \
            >> "${raw_dir}/cli/compat.tsv"
        echo "    ${tool} ${op}: FAIL ($(_tsv_field "${reason}"))"
        return 0
    fi
    rm -f "${raw_dir}/cli/.hyperfine.err"
    printf '%s\t%s\t%s\tpass\t\n' "${tool}" "${fid}" "${op}" >> "${raw_dir}/cli/compat.tsv"
    printf '%s\t%s\t%s\t%s\n' "${tool}" "${fid}" "${op}" "${rel}" >> "${raw_dir}/cli/index.tsv"
    echo "    ${tool} ${op}: timed"
}

# _cli_failed <raw_dir> <tool> <fixture id> <operation>: true if compat.tsv has a fail row for it.
_cli_failed() {
    awk -F '\t' -v t="$2" -v f="$3" -v o="$4" \
        '$1 == t && $2 == f && $3 == o && $4 == "fail" { found = 1 } END { exit !found }' \
        "$1/cli/compat.tsv"
}

# run_cli_tier <raw_dir>: time every (fixture, operation, tool) cell's CLI with hyperfine, writing
# <raw_dir>/cli/{compat.tsv,index.tsv,*.json}. Expects tools.sh sourced, the release binaries
# built, and BENCH_FIXTURE_ROOTS exported.
run_cli_tier() {
    local raw_dir="$1"
    local work fid fdir external schema valid invalid tool op out cmd fixtures
    mkdir -p "${raw_dir}/cli"
    : > "${raw_dir}/cli/compat.tsv"
    : > "${raw_dir}/cli/index.tsv"
    # Detect the offline mechanism once rather than per probe.
    BENCH_OFFLINE_WRAPPER="$(_offline_wrapper)"
    export BENCH_OFFLINE_WRAPPER
    if [[ -z "${BENCH_OFFLINE_WRAPPER}" ]]; then
        echo "!! neither sandbox-exec nor unshare is available: CLI probes cannot deny network" >&2
        echo "   access, so a tool that downloads remote documents may have the download timed." >&2
    fi
    # Capture the list first: a failure inside `done < <(...)` would escape `set -e` and leave
    # the CLI tier silently empty.
    fixtures="$(discover_fixtures)"
    work="$(mktemp -d "${TMPDIR:-/tmp}/jsonschemars-bench-cli.XXXXXX")"
    # shellcheck disable=SC2064  # expand ${work} now: it is local and gone when the trap fires.
    trap "rm -rf '${work}'" EXIT

    while IFS="$(printf '\t')" read -r fid fdir external; do
        [[ -n "${fid}" ]] || continue
        echo "==> CLI tier: ${fid}"
        if [[ -n "${external}" ]]; then
            for tool in $(codegen_tools); do
                _cli_offline_skip "${raw_dir}" "${tool}" "${fid}" codegen "${external}"
            done
            for tool in $(validation_tools); do
                for op in validate_valid validate_invalid; do
                    _cli_offline_skip "${raw_dir}" "${tool}" "${fid}" "${op}" "${external}"
                done
            done
            continue
        fi
        schema="${fdir}/schema.json"
        valid="$(_first_instance "${fdir}/instances/valid")"
        invalid="$(_first_instance "${fdir}/instances/invalid")"

        for tool in $(codegen_tools); do
            out="${work}/${tool}__${fid//\//__}"
            mkdir -p "${out}"
            cmd="$(codegen_cmd "${tool}" "${schema}" "${out}")"
            _cli_measure "${raw_dir}" "${tool}" "${fid}" codegen zero "${cmd}"
        done

        for tool in $(validation_tools); do
            if [[ -n "${valid}" ]]; then
                cmd="$(validate_cmd "${tool}" "${schema}" "${valid}")"
                _cli_measure "${raw_dir}" "${tool}" "${fid}" validate_valid zero "${cmd}"
            fi
            if [[ -z "${invalid}" ]]; then
                continue
            fi
            # A validator that rejects the valid instance cannot handle this schema (e.g. it fails
            # to compile it), so its non-zero exit on the invalid instance proves nothing.
            if [[ -n "${valid}" ]] && _cli_failed "${raw_dir}" "${tool}" "${fid}" validate_valid; then
                printf '%s\t%s\t%s\tfail\t%s\n' "${tool}" "${fid}" validate_invalid \
                    "not measured: valid instance was rejected" >> "${raw_dir}/cli/compat.tsv"
                echo "    ${tool} validate_invalid: FAIL (not measured: valid instance was rejected)"
                continue
            fi
            cmd="$(validate_cmd "${tool}" "${schema}" "${invalid}")"
            _cli_measure "${raw_dir}" "${tool}" "${fid}" validate_invalid nonzero "${cmd}"
        done
    done <<< "${fixtures}"

    rm -rf "${work}"
    trap - EXIT
}

# _locked_version <crate>: the version Cargo.lock resolved for <crate> ("unknown" if absent; the
# first entry when several versions are locked).
_locked_version() {
    awk -v name="$1" '
        $0 == "name = \"" name "\"" { getline; gsub(/version = |"/, ""); print; found = 1; exit }
        END { if (!found) print "unknown" }' "${REPO_ROOT}/Cargo.lock"
}

# _latest_stable <crate>: the newest non-prerelease version of <crate> on crates.io ("" when the
# lookup fails, e.g. offline).
_latest_stable() {
    curl -fsS --max-time 10 -A 'json-schema-rs-benchmark' "https://crates.io/api/v1/crates/$1" 2>/dev/null \
        | grep -o '"max_stable_version":"[^"]*"' | head -n 1 | cut -d '"' -f 4
}

# check_competitor_versions: fail unless every competitor is its newest stable release, so the
# harness never measures against a target that has already moved. Each linked crate (Cargo.lock)
# must equal the newest stable on crates.io, and each CLI must equal the crate it ships with, so
# both tiers measure the same release. When crates.io is unreachable the freshness half is
# skipped with a warning; the CLI-matches-crate half always runs.
check_competitor_versions() {
    local stale=0 pair crate cli locked latest installed
    # <linked crate>:<CLI binary of the same release, or "-" when the tool ships none>
    for pair in jsonschema:jsonschema-cli typify:cargo-typify boon:-; do
        crate="${pair%%:*}"
        cli="${pair#*:}"
        locked="$(_locked_version "${crate}")"
        latest="$(_latest_stable "${crate}")"
        if [[ -z "${latest}" ]]; then
            echo "  WARNING: could not look up the newest ${crate} on crates.io; freshness not checked" >&2
        elif [[ "${locked}" != "${latest}" ]]; then
            echo "  STALE: ${crate} ${locked} is linked; newest stable is ${latest}" \
                "(bump json_schema_rs_benchmark/Cargo.toml, then: cargo update -p ${crate})" >&2
            stale=1
        fi
        [[ "${cli}" == "-" ]] && continue
        installed="$(tool_version "${cli}")"
        if [[ "${installed}" != "${latest:-${locked}}" ]]; then
            echo "  STALE: ${cli} ${installed} is installed; expected ${latest:-${locked}}" \
                "(install: cargo install ${cli}@${latest:-${locked}} --locked)" >&2
            stale=1
        fi
    done
    return "${stale}"
}

# --- main --------------------------------------------------------------------------------

main() {
    echo "==> json-schema-rs benchmark harness"
    echo "    crate: ${CRATE_DIR}"

    # 1. Required tools. A partial run would misrepresent the comparison, so abort instead.
    echo "==> checking required external tools"
    if ! check_required_tools; then
        echo "!! required tools are missing (see above); aborting." >&2
        exit 1
    fi
    echo "==> checking competitors are their newest stable releases"
    if ! check_competitor_versions; then
        echo "!! out-of-date competitors (see above); upgrade them and re-run." >&2
        exit 1
    fi

    # 2. Build.
    echo "==> building release binaries"
    cargo build --release -p json-schema-rs --bin jsonschemars
    cargo build --release -p json-schema-rs-benchmark --bins

    # 3. Synthetic fixtures.
    echo "==> generating synthetic fixtures"
    "${RELEASE_DIR}/gen_fixtures" --output "${GENERATED_FIXTURES}"
    export BENCH_FIXTURE_ROOTS="${COMMITTED_FIXTURES}:${GENERATED_FIXTURES}"

    # 4. Fresh raw captures; stale criterion benches must never leak into the results.
    rm -rf "${RAW_DIR}" "${CRITERION_DIR}"
    mkdir -p "${RAW_DIR}"
    local tool
    for tool in $(required_external_tools) rustc; do
        printf '%s\t%s\n' "${tool}" "$(tool_version "${tool}")"
    done > "${RAW_DIR}/tool_versions.tsv"
    # The in-process tier links these crates; record the versions Cargo.lock resolved.
    for tool in jsonschema boon typify json-schema-rs; do
        printf '%s\t%s\n' "${tool}" "$(_locked_version "${tool}")"
    done >> "${RAW_DIR}/tool_versions.tsv"

    # 5. In-process peak heap + compat.
    echo "==> in-process tier: peak heap + compat"
    "${RELEASE_DIR}/heap_profile" --output "${RAW_DIR}"

    # 6. In-process timing.
    if [[ "${BENCH_SKIP_CRITERION:-0}" == "1" ]]; then
        echo "==> in-process tier: skipping criterion (BENCH_SKIP_CRITERION=1)"
    else
        echo "==> in-process tier: criterion timing"
        local -a criterion_args
        # Word-split the knob on purpose: it is a list of criterion flags.
        # shellcheck disable=SC2206
        criterion_args=(${BENCH_CRITERION_ARGS:---warm-up-time 0.5 --measurement-time 2 --sample-size 20})
        # `+` guard: an empty array under `set -u` is an unbound-variable error on bash 3.2.
        cargo bench -p json-schema-rs-benchmark --bench cross_tool -- ${criterion_args[@]+"${criterion_args[@]}"}
    fi

    # 7. CLI timing.
    echo "==> CLI tier: hyperfine (warmup ${BENCH_CLI_WARMUP:-3}, runs ${BENCH_CLI_RUNS:-15})"
    run_cli_tier "${RAW_DIR}"

    # 8. Aggregate.
    echo "==> aggregating results"
    "${RELEASE_DIR}/aggregate" \
        --input "${RAW_DIR}" \
        --criterion "${CRITERION_DIR}" \
        --output "${CRATE_DIR}"

    echo "==> done. results written to:"
    echo "    ${CRATE_DIR}/results.json"
    echo "    ${CRATE_DIR}/results.md"
}

# Run only when executed, so the functions above can be sourced and exercised on their own.
if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
    main "$@"
fi
