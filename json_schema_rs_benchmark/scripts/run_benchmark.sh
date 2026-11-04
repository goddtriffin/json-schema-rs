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
#     validate_invalid, reverse_codegen. Tools: jsonschemars (our lib), jsonschema, boon, valico,
#     jsonschema-valid, typify, schemafy, jsonschema_code_generator; and, from isolated/ (its own
#     workspace, built once per feature so their dependency features never reach the other
#     tools), corvus-json-schema, jsonschema-macros (compile-time validators generated from the
#     fixture set) and the reverse_codegen models (our codegen's Rust types for every fixture with
#     both `#[derive(ToJsonSchema)]` and schemars' `#[derive(JsonSchema)]`; a model whose derive
#     does not compile is excluded and recorded as a compat failure, see build_reverse_models).
#     Every build appends to the same raw captures.
#
#   CLI tier — each tool's command-line front end timed end to end (startup, file I/O, parse,
#     compile, generate/validate, exit) by hyperfine, with warmup and repeated runs; hyperfine
#     also records each run's peak RSS. Operations: codegen, validate_valid, validate_invalid,
#     reverse_codegen (language-native models -> JSON Schema: jsonschemars and schemars through
#     the reverse_schema bin, pydantic, zod, ts-json-schema-generator; each non-Rust tool's models
#     are generated from the fixture before timing, never timed or committed). Tools and their
#     invocations are defined in scripts/tools.sh.
#
# Every Rust tool, ours and the competitors' alike (linked crates, the isolated builds, and the
# competitor CLIs, which the driver installs), is built with one profile: `benchmark` in
# profile.toml (one codegen unit and thin LTO), passed to every cargo command with --config. With
# Cargo's default 16 codegen units, which unit a hot function lands in, and so what is inlined into
# it, shifts when unrelated code is added to a crate, which moved cells by ~10% with no code path
# changed. The profile is recorded in the results (the rustc row of the Tools table).
#
# A tool is only measured on a fixture/operation it handles CORRECTLY (codegen succeeds; valid
# instance accepted; invalid instance rejected; reverse codegen emits a schema our validator
# accepts that accepts every valid instance of the fixture). Anything else is recorded as a compat failure and
# left untimed, so a fast failure never masquerades as a fast result. The run is hermetic: a
# fixture may vendor the documents its schema references (resources/*.json, registered under their
# root $id with every tool that can take them; a tool that cannot is a compat failure), a fixture
# with a $ref to any document it does not vendor is not measured, and each CLI correctness probe runs
# with network access denied (macOS sandbox-exec / Linux unshare -rn), so a tool that downloads a
# remote document fails its probe instead of having the download timed. A tool missing from the
# machine is different: the driver aborts up front rather than silently omit a competitor.
#
# Pipeline:
#   1. Check required external tools, install the non-Rust competitors from their lock files
#      (competitors/node: npm ci; competitors/python: uv sync), and check that every competitor is
#      its newest stable release (linked crate == crates.io's newest stable; npm / PyPI package ==
#      the registry's latest), so a run never measures against an outdated target. Install the
#      Rust competitor CLIs (jsonschema-cli, cargo-typify) at the release the benchmark links.
#      Record the versions and runtimes.
#   2. Build the benchmark-profile binaries (jsonschemars CLI + the benchmark crate's bins; the
#      isolated bins, reverse_schema among them, are built per feature in step 5).
#   3. Generate the synthetic fixtures (target/benchmark-fixtures; deterministic, not committed).
#   4. Reset the raw capture dir and criterion's output dir.
#   5. In-process peak heap + compat matrix (heap_profile, then isolated_heap_profile per feature).
#   6. In-process timing (criterion: cross_tool, then the isolated bench per feature).
#   7. CLI timing (hyperfine) + CLI compat matrix.
#   8. Aggregate everything into the committed results.json + results.md.
#
# Raw captures (RAW_DIR = json_schema_rs_benchmark/target/benchmark-raw):
#   tools.tsv                         tool \t version \t language/runtime
#   reverse_exclude.tsv               tool \t fixture \t reason: reverse_codegen models excluded
#                                     from the isolated build (the model does not compile)
#   reverse_build.jsonl               cargo's JSON messages from the last reverse-codegen check
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
# The build profile of every Rust tool (see the header), and where it puts its binaries.
PROFILE_CONFIG="${CRATE_DIR}/profile.toml"
BUILD_DIR="${REPO_ROOT}/target/benchmark"
ISOLATED_MANIFEST="${CRATE_DIR}/isolated/Cargo.toml"
ISOLATED_LOCK="${CRATE_DIR}/isolated/Cargo.lock"
# One build per feature: each competitor here must not share a build with the others.
ISOLATED_FEATURES="corvus jsonschema-macros reverse-codegen"
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

# discover_fixtures: print "<fixture id>\t<fixture dir>\t<resource count>\t<unresolved refs>" for
# every fixture under BENCH_FIXTURE_ROOTS, sorted by id, using the in-process tier's discovery
# (list_fixtures) so both tiers measure the same set. <unresolved refs> lists the $refs naming a
# document the fixture does not vendor (empty when every one resolves).
discover_fixtures() {
    "${BUILD_DIR}/list_fixtures"
}

# _resources <fixture dir>: the fixture's resource documents (resources/*.json), one per line,
# sorted by file name like the in-process tier's discovery.
_resources() {
    [[ -d "$1/resources" ]] || return 0
    find "$1/resources" -mindepth 1 -maxdepth 1 -type f -name '*.json' | LC_ALL=C sort
}

# _cli_offline_skip <raw_dir> <tool> <fixture id> <operation> <unresolved refs>: record a cell left
# unmeasured because the schema needs documents the fixture does not vendor. The benchmark runs
# offline, so timing it would measure a failed lookup or a network fetch rather than the operation.
# The detail text matches the in-process tier's (adapters::OFFLINE_SKIP).
_cli_offline_skip() {
    printf '%s\t%s\t%s\tfail\t%s\n' "$2" "$3" "$4" \
        "not measured: schema needs external documents the fixture does not vendor (offline run)" \
        >> "$1/cli/compat.tsv"
    echo "    $2 $4: skipped (schema needs external documents: ${5//,/, })"
}

# _cli_resources_unsupported <raw_dir> <tool> <fixture id> <operation>: record a cell whose tool's
# CLI cannot take the fixture's resource documents. Running it without them would measure a failed
# lookup. The detail text matches the in-process tier's (adapters::RESOURCES_UNSUPPORTED).
_cli_resources_unsupported() {
    printf '%s\t%s\t%s\tfail\t%s\n' "$2" "$3" "$4" \
        "cannot load the fixture's resource documents" >> "$1/cli/compat.tsv"
    echo "    $2 $4: FAIL (cannot load the fixture's resource documents)"
}

# _cli_fail <raw_dir> <tool> <fixture id> <operation> <reason>: record a compat failure.
_cli_fail() {
    printf '%s\t%s\t%s\tfail\t%s\n' "$2" "$3" "$4" "$(_tsv_field "$5")" >> "$1/cli/compat.tsv"
    echo "    $2 $4: FAIL ($(_tsv_field "$5"))"
}

# _cli_measure <raw_dir> <tool> <fixture id> <operation> <expect: zero|nonzero> <command line>
# Probe the command once for correctness, offline; on a correct result time it with hyperfine and index the
# export, otherwise record a compat failure and skip timing.
_cli_measure() {
    local raw_dir="$1" tool="$2" fid="$3" op="$4" expect="$5" cmd="$6"
    local rc=0 err ok reason

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
        _cli_fail "${raw_dir}" "${tool}" "${fid}" "${op}" "${reason}"
        return 0
    fi
    _cli_time "${raw_dir}" "${tool}" "${fid}" "${op}" "${expect}" "${cmd}"
}

# _cli_time <raw_dir> <tool> <fixture id> <operation> <expect: zero|nonzero> <command line>: time a
# probe-correct command with hyperfine and index the export (a failed run is a compat failure).
_cli_time() {
    local raw_dir="$1" tool="$2" fid="$3" op="$4" expect="$5" cmd="$6"
    local reason rel json
    local -a hf_args
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
        _cli_fail "${raw_dir}" "${tool}" "${fid}" "${op}" "${reason}"
        return 0
    fi
    rm -f "${raw_dir}/cli/.hyperfine.err"
    printf '%s\t%s\t%s\tpass\t\n' "${tool}" "${fid}" "${op}" >> "${raw_dir}/cli/compat.tsv"
    printf '%s\t%s\t%s\t%s\n' "${tool}" "${fid}" "${op}" "${rel}" >> "${raw_dir}/cli/index.tsv"
    echo "    ${tool} ${op}: timed"
}

# _cli_reverse <raw_dir> <work dir> <tool> <fixture id> <fixture dir> [resource...]: one
# reverse_codegen cell. Generate the tool's models from the fixture's schema (untimed, offline),
# run the timed command once offline and capture the schema it prints, check that schema with our
# validator against every valid instance of the fixture, and only then time the command. A failed
# generation, a failed run, a schema our validator does not accept or one that rejects a valid
# instance is a compat failure and is not timed.
_cli_reverse() {
    local raw_dir="$1" work="$2" tool="$3" fid="$4" fdir="$5"
    local model_dir gen cmd emitted rc=0 err instance checked=0 out
    shift 5
    model_dir="$(reverse_model_dir "${tool}" "${fid}")"
    gen=""
    if [[ -n "${model_dir}" ]]; then
        rm -rf "${model_dir}"
        mkdir -p "${model_dir}"
        gen="$(reverse_model_cmd "${tool}" "${fdir}/schema.json" "${model_dir}" ${1+"$@"})"
    fi
    if [[ -n "${gen}" ]]; then
        err="$(run_cmd_offline "${gen}" 2>&1)" || rc=$?
        if [[ "${rc}" -ne 0 ]]; then
            _cli_fail "${raw_dir}" "${tool}" "${fid}" reverse_codegen \
                "model generation failed: $(printf '%s\n' "${err:-exit ${rc}}" | _failure_summary)"
            return 0
        fi
        if ! err="$(reverse_model_check "${tool}" "${model_dir}")"; then
            _cli_fail "${raw_dir}" "${tool}" "${fid}" reverse_codegen "model generation failed: ${err}"
            return 0
        fi
    fi

    cmd="$(reverse_codegen_cmd "${tool}" "${fid}" "${model_dir}")"
    emitted="${work}/${tool}__${fid//\//__}__reverse_codegen.json"
    rc=0
    run_cmd_offline "${cmd}" > "${emitted}" 2> "${emitted}.err" || rc=$?
    if [[ "${rc}" -ne 0 ]]; then
        _cli_fail "${raw_dir}" "${tool}" "${fid}" reverse_codegen \
            "exit ${rc}: $(_failure_summary < "${emitted}.err")"
        return 0
    fi
    # The emitted schema must accept every valid instance (exit 0); exit 1 is a rejection, any
    # other status means our validator could not use the schema at all.
    while IFS= read -r instance; do
        [[ -n "${instance}" ]] || continue
        checked=1
        rc=0
        out="$("$(_jsonschemars_bin)" validate -s "${emitted}" -p "${instance}" 2>&1)" || rc=$?
        if [[ "${rc}" -eq 1 ]]; then
            _cli_fail "${raw_dir}" "${tool}" "${fid}" reverse_codegen \
                "emitted schema rejects valid instance $(basename "${instance}")"
            return 0
        elif [[ "${rc}" -ne 0 ]]; then
            _cli_fail "${raw_dir}" "${tool}" "${fid}" reverse_codegen \
                "emitted schema is not a schema our validator accepts: $(printf '%s\n' "${out}" | _failure_summary)"
            return 0
        fi
    done <<< "$(find "${fdir}/instances/valid" -mindepth 1 -maxdepth 1 -type f -name '*.json' 2>/dev/null | LC_ALL=C sort)"
    if [[ "${checked}" -eq 0 ]]; then
        _cli_fail "${raw_dir}" "${tool}" "${fid}" reverse_codegen \
            "not measured: the fixture has no valid instance to check the emitted schema against"
        return 0
    fi
    _cli_time "${raw_dir}" "${tool}" "${fid}" reverse_codegen zero "${cmd}"
}

# _write_startup_fixture <work dir>: write the startup-baseline fixture (a draft-07 object schema
# with one optional string property, valid instance `{}`, invalid instance `[]`) under <work dir>
# and print its directory. Draft-07 because every tool supports it; the property because a model
# generator needs it to name a root type (quicktype names none for a bare `{"type": "object"}`).
# The reverse_schema binary builds in the same schema (STARTUP_SCHEMA in src/reverse_models.rs).
_write_startup_fixture() {
    local dir="$1/startup"
    mkdir -p "${dir}/instances/valid" "${dir}/instances/invalid"
    printf '%s\n' '{"$schema": "http://json-schema.org/draft-07/schema#", "type": "object", "properties": {"name": {"type": "string"}}}' \
        > "${dir}/schema.json"
    printf '{}\n' > "${dir}/instances/valid/object.json"
    printf '[]\n' > "${dir}/instances/invalid/array.json"
    printf '%s\n' "${dir}"
}

# _cli_failed <raw_dir> <tool> <fixture id> <operation>: true if compat.tsv has a fail row for it.
_cli_failed() {
    awk -F '\t' -v t="$2" -v f="$3" -v o="$4" \
        '$1 == t && $2 == f && $3 == o && $4 == "fail" { found = 1 } END { exit !found }' \
        "$1/cli/compat.tsv"
}

# run_cli_tier <raw_dir>: time every (fixture, operation, tool) cell's CLI with hyperfine, writing
# <raw_dir>/cli/{compat.tsv,index.tsv,*.json}. Expects tools.sh sourced, the benchmark-profile binaries
# built, and BENCH_FIXTURE_ROOTS exported.
run_cli_tier() {
    local raw_dir="$1"
    local work fid fdir unresolved resource_count schema valid invalid tool op out cmd fixtures
    local resource
    local -a resources
    mkdir -p "${raw_dir}/cli"
    : > "${raw_dir}/cli/compat.tsv"
    : > "${raw_dir}/cli/index.tsv"
    # Generated reverse_codegen models are per run: never reuse a previous run's.
    rm -rf "$(_python_dir)/reverse-models" "$(_node_dir)/reverse-models"
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
    # The startup baseline (STARTUP_FIXTURE in src/lib.rs) leads the list: every tool on a trivial
    # input, so each tool's fixed per-process cost can be told apart from the work.
    fixtures="$(printf 'startup\t%s\t0\t\n' "$(_write_startup_fixture "${work}")")
${fixtures}"

    while IFS="$(printf '\t')" read -r fid fdir resource_count unresolved; do
        [[ -n "${fid}" ]] || continue
        echo "==> CLI tier: ${fid}"
        if [[ -n "${unresolved}" ]]; then
            for tool in $(codegen_tools); do
                _cli_offline_skip "${raw_dir}" "${tool}" "${fid}" codegen "${unresolved}"
            done
            for tool in $(validation_tools); do
                for op in validate_valid validate_invalid; do
                    _cli_offline_skip "${raw_dir}" "${tool}" "${fid}" "${op}" "${unresolved}"
                done
            done
            for tool in $(reverse_codegen_tools); do
                _cli_offline_skip "${raw_dir}" "${tool}" "${fid}" reverse_codegen "${unresolved}"
            done
            continue
        fi
        schema="${fdir}/schema.json"
        valid="$(_first_instance "${fdir}/instances/valid")"
        invalid="$(_first_instance "${fdir}/instances/invalid")"
        resources=()
        while IFS= read -r resource; do
            [[ -n "${resource}" ]] && resources+=("${resource}")
        done <<< "$(_resources "${fdir}")"
        if [[ "${#resources[@]}" -ne "${resource_count:-0}" ]]; then
            echo "!! ${fid}: found ${#resources[@]} resources, list_fixtures reported ${resource_count:-0}" >&2
            exit 1
        fi

        for tool in $(codegen_tools); do
            if [[ "${#resources[@]}" -gt 0 ]] && ! loads_resources "${tool}" codegen; then
                _cli_resources_unsupported "${raw_dir}" "${tool}" "${fid}" codegen
                continue
            fi
            out="${work}/${tool}__${fid//\//__}"
            mkdir -p "${out}"
            cmd="$(codegen_cmd "${tool}" "${schema}" "${out}" ${resources[@]+"${resources[@]}"})"
            _cli_measure "${raw_dir}" "${tool}" "${fid}" codegen zero "${cmd}"
        done

        for tool in $(validation_tools); do
            if [[ "${#resources[@]}" -gt 0 ]] && ! loads_resources "${tool}" validate_valid; then
                for op in validate_valid validate_invalid; do
                    _cli_resources_unsupported "${raw_dir}" "${tool}" "${fid}" "${op}"
                done
                continue
            fi
            if [[ -n "${valid}" ]]; then
                cmd="$(validate_cmd "${tool}" "${schema}" "${valid}" ${resources[@]+"${resources[@]}"})"
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
            cmd="$(validate_cmd "${tool}" "${schema}" "${invalid}" ${resources[@]+"${resources[@]}"})"
            _cli_measure "${raw_dir}" "${tool}" "${fid}" validate_invalid nonzero "${cmd}"
        done

        for tool in $(reverse_codegen_tools); do
            if [[ "${#resources[@]}" -gt 0 ]] && ! loads_resources "${tool}" reverse_codegen; then
                _cli_resources_unsupported "${raw_dir}" "${tool}" "${fid}" reverse_codegen
                continue
            fi
            _cli_reverse "${raw_dir}" "${work}" "${tool}" "${fid}" "${fdir}" ${resources[@]+"${resources[@]}"}
        done
    done <<< "${fixtures}"

    rm -rf "${work}"
    trap - EXIT
}

# _locked_version <crate> [lock file]: the version a lock file (default: the workspace's
# Cargo.lock) resolved for <crate> ("unknown" if absent; the first entry when several versions are
# locked).
_locked_version() {
    awk -v name="$1" '
        $0 == "name = \"" name "\"" { getline; gsub(/version = |"/, ""); print; found = 1; exit }
        END { if (!found) print "unknown" }' "${2:-${REPO_ROOT}/Cargo.lock}"
}

# _profile_cargo <subcommand> [cargo args...]: run a cargo build or bench with the benchmark
# profile. The profile goes right after the subcommand, ahead of any `--`.
_profile_cargo() {
    local subcommand="$1"
    shift
    cargo "${subcommand}" --config "${PROFILE_CONFIG}" --profile benchmark "$@"
}

# _isolated_cargo <subcommand> [cargo args...]: run a cargo subcommand on isolated/ with the
# workspace's target directory, so its criterion output lands next to the main crate's (and its
# builds reuse the same disk). The manifest goes right after the subcommand, ahead of any `--`.
_isolated_cargo() {
    local subcommand="$1"
    shift
    CARGO_TARGET_DIR="${REPO_ROOT}/target" cargo "${subcommand}" \
        --manifest-path "${ISOLATED_MANIFEST}" "$@"
}

# _latest_stable <crate>: the newest non-prerelease version of <crate> on crates.io ("" when the
# lookup fails, e.g. offline).
_latest_stable() {
    curl -fsS --max-time 10 -A 'json-schema-rs-benchmark' "https://crates.io/api/v1/crates/$1" 2>/dev/null \
        | grep -o '"max_stable_version":"[^"]*"' | head -n 1 | cut -d '"' -f 4
}

# check_competitor_versions: fail unless every competitor is its newest stable release, so the
# harness never measures against a target that has already moved. Each linked crate (Cargo.lock)
# must equal the newest stable on crates.io (the Rust competitor CLIs are then installed at that
# release, so both tiers measure the same one). When crates.io is unreachable the check is skipped
# with a warning.
check_competitor_versions() {
    local stale=0 pair crate locked latest lock manifest
    # The isolated crate's lock file is resolved on its first build; resolve it now if absent.
    if [[ ! -f "${ISOLATED_LOCK}" ]]; then
        _isolated_cargo generate-lockfile
    fi
    # <linked crate>:<crate linked from isolated/ ("isolated") or from the main benchmark crate
    # ("main")>
    for pair in jsonschema:main typify:main boon:main valico:main jsonschema-valid:main \
        schemafy_lib:main jsonschema_code_generator:main corvus-json-schema:isolated \
        jsonschema:isolated schemars:isolated; do
        crate="${pair%%:*}"
        if [[ "${pair##*:}" == "isolated" ]]; then
            lock="${ISOLATED_LOCK}"
            manifest="json_schema_rs_benchmark/isolated/Cargo.toml"
        else
            lock="${REPO_ROOT}/Cargo.lock"
            manifest="json_schema_rs_benchmark/Cargo.toml"
        fi
        locked="$(_locked_version "${crate}" "${lock}")"
        latest="$(_latest_stable "${crate}")"
        if [[ -z "${latest}" ]]; then
            echo "  WARNING: could not look up the newest ${crate} on crates.io; freshness not checked" >&2
        elif [[ "${locked}" != "${latest}" ]]; then
            echo "  STALE: ${crate} ${locked} is linked; newest stable is ${latest}" \
                "(bump ${manifest}, then: cargo update -p ${crate} --manifest-path ${manifest})" >&2
            stale=1
        fi
    done
    # Non-Rust competitors: the lock file's pin must equal the registry's latest release.
    local package
    for package in @sourcemeta/jsonschema ajv ajv-draft-04 quicktype json-schema-to-typescript zod \
        ts-json-schema-generator; do
        locked="$(npm_locked_version "${package}")"
        latest="$(npm view "${package}" version 2>/dev/null || true)"
        if [[ -z "${latest}" ]]; then
            echo "  WARNING: could not look up the newest ${package} on npm; freshness not checked" >&2
        elif [[ "${locked}" != "${latest}" ]]; then
            echo "  STALE: npm ${package} ${locked} is pinned; newest is ${latest}" \
                "(in json_schema_rs_benchmark/competitors/node: npm install --save-exact ${package}@${latest})" >&2
            stale=1
        fi
    done
    for package in jsonschema fastjsonschema datamodel-code-generator pydantic; do
        locked="$(pypi_locked_version "${package}")"
        latest="$(_pypi_latest "${package}")"
        if [[ -z "${latest}" ]]; then
            echo "  WARNING: could not look up the newest ${package} on PyPI; freshness not checked" >&2
        elif [[ "${locked}" != "${latest}" ]]; then
            echo "  STALE: PyPI ${package} ${locked} is pinned; newest is ${latest}" \
                "(in json_schema_rs_benchmark/competitors/python: uv add ${package}==${latest})" >&2
            stale=1
        fi
    done
    return "${stale}"
}

# _pypi_latest <package>: the newest release of <package> on PyPI ("" when the lookup fails).
_pypi_latest() {
    curl -fsS --max-time 10 "https://pypi.org/pypi/$1/json" 2>/dev/null \
        | "$(_python_bin)" -c 'import json, sys; print(json.load(sys.stdin)["info"]["version"])' \
            2>/dev/null || true
}

# _tool_row <id> <version>: one tools.tsv row (id, version, language/runtime).
_tool_row() {
    printf '%s\t%s\t%s\n' "$1" "$2" "$(tool_runtime "$1")"
}

# build_reverse_models: build isolated/ with the reverse-codegen feature (every bin, reverse_schema
# among them), excluding each model that does not compile. Our codegen emits every fixture's Rust
# types, one copy per tool with that tool's derive; a model that does not compile is that tool's
# compat failure on that fixture, not a broken build. `cargo check` finds them (no code
# generation, so each round is fast): after each failed check, model_build_failures reads cargo's
# JSON messages and appends one "tool \t fixture \t reason" row per failing model to
# BENCH_REVERSE_EXCLUDE, which the isolated build script reads to leave those models out. An error
# it cannot attribute to one model aborts the run. Once the check passes, one benchmark-profile build
# follows. BENCH_REVERSE_EXCLUDE stays exported so the later criterion build sees the same
# exclusions.
build_reverse_models() {
    local attempt before after
    export BENCH_REVERSE_EXCLUDE="${RAW_DIR}/reverse_exclude.tsv"
    : > "${BENCH_REVERSE_EXCLUDE}"
    for attempt in 1 2 3 4 5; do
        if _isolated_cargo check --config "${PROFILE_CONFIG}" --profile benchmark \
            --features reverse-codegen --bins \
            --message-format=json > "${RAW_DIR}/reverse_build.jsonl"; then
            if [[ -s "${BENCH_REVERSE_EXCLUDE}" ]]; then
                echo "    excluded reverse_codegen models (model does not compile):"
                sed 's/^/      /' "${BENCH_REVERSE_EXCLUDE}"
            fi
            _isolated_cargo build --config "${PROFILE_CONFIG}" --profile benchmark \
                --features reverse-codegen --bins
            return 0
        fi
        echo "    reverse-codegen check ${attempt} failed; excluding the models that did not compile"
        before="$(wc -l < "${BENCH_REVERSE_EXCLUDE}")"
        if ! "${BUILD_DIR}/model_build_failures" < "${RAW_DIR}/reverse_build.jsonl" \
            >> "${BENCH_REVERSE_EXCLUDE}" 2> "${RAW_DIR}/model_build_failures.err"; then
            echo "!! the reverse-codegen check failed for a reason no single model explains; aborting:" >&2
            cat "${RAW_DIR}/model_build_failures.err" >&2
            exit 1
        fi
        after="$(wc -l < "${BENCH_REVERSE_EXCLUDE}")"
        if [[ "${after}" -le "${before}" ]]; then
            echo "!! the reverse-codegen check failed but no new model was excluded; aborting" \
                "(see ${RAW_DIR}/reverse_build.jsonl)" >&2
            exit 1
        fi
    done
    echo "!! the reverse-codegen check still fails after 5 attempts; aborting" \
        "(see ${RAW_DIR}/reverse_build.jsonl)" >&2
    exit 1
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
    echo "==> installing non-Rust competitors from their lock files"
    if ! install_competitor_envs; then
        echo "!! installing the Node or Python competitors failed (see above); aborting." >&2
        exit 1
    fi
    echo "==> checking competitors are their newest stable releases"
    if ! check_competitor_versions; then
        echo "!! out-of-date competitors (see above); upgrade them and re-run." >&2
        exit 1
    fi
    echo "==> installing the Rust competitor CLIs (benchmark profile: $(profile_summary))"
    if ! install_rust_cli_competitors "$(_locked_version jsonschema)" "$(_locked_version typify)"; then
        echo "!! installing jsonschema-cli or cargo-typify failed (see above); aborting." >&2
        exit 1
    fi

    # 2. Build.
    echo "==> building benchmark-profile binaries ($(profile_summary))"
    _profile_cargo build -p json-schema-rs --bin jsonschemars
    _profile_cargo build -p json-schema-rs-benchmark --bins

    # 3. Synthetic fixtures.
    echo "==> generating synthetic fixtures"
    "${BUILD_DIR}/gen_fixtures" --output "${GENERATED_FIXTURES}"
    export BENCH_FIXTURE_ROOTS="${COMMITTED_FIXTURES}:${GENERATED_FIXTURES}"

    # 4. Fresh raw captures; stale criterion benches must never leak into the results.
    rm -rf "${RAW_DIR}" "${CRITERION_DIR}"
    mkdir -p "${RAW_DIR}"
    # Every tool under the identifier its measurements use, with its release and runtime. Linked
    # crates come from Cargo.lock (the CLIs are installed at the same releases and report their
    # own), the non-Rust competitors from their lock files.
    {
        _tool_row jsonschemars "$(_locked_version json-schema-rs)"
        _tool_row jsonschema "$(_locked_version jsonschema)"
        _tool_row jsonschema-cli "$(tool_version jsonschema-cli)"
        _tool_row boon "$(_locked_version boon)"
        _tool_row typify "$(_locked_version typify)"
        _tool_row valico "$(_locked_version valico)"
        _tool_row jsonschema-valid "$(_locked_version jsonschema-valid)"
        _tool_row schemafy "$(_locked_version schemafy_lib)"
        _tool_row jsonschema_code_generator "$(_locked_version jsonschema_code_generator)"
        _tool_row corvus-json-schema "$(_locked_version corvus-json-schema "${ISOLATED_LOCK}")"
        _tool_row jsonschema-macros "$(_locked_version jsonschema "${ISOLATED_LOCK}")"
        _tool_row schemars "$(_locked_version schemars "${ISOLATED_LOCK}")"
        _tool_row blaze "$(npm_locked_version @sourcemeta/jsonschema)"
        _tool_row ajv "$(npm_locked_version ajv)"
        _tool_row quicktype "$(npm_locked_version quicktype)"
        _tool_row json-schema-to-typescript "$(npm_locked_version json-schema-to-typescript)"
        _tool_row zod "$(npm_locked_version zod)"
        _tool_row ts-json-schema-generator "$(npm_locked_version ts-json-schema-generator)"
        _tool_row datamodel-code-generator "$(pypi_locked_version datamodel-code-generator)"
        _tool_row python-jsonschema "$(pypi_locked_version jsonschema)"
        _tool_row fastjsonschema "$(pypi_locked_version fastjsonschema)"
        _tool_row pydantic "$(pypi_locked_version pydantic)"
        _tool_row hyperfine "$(tool_version hyperfine)"
        _tool_row rustc "$(tool_version rustc)"
    } > "${RAW_DIR}/tools.tsv"

    # 5. In-process peak heap + compat.
    echo "==> in-process tier: peak heap + compat"
    "${BUILD_DIR}/heap_profile" --output "${RAW_DIR}"
    local feature
    for feature in ${ISOLATED_FEATURES}; do
        echo "==> in-process tier: peak heap + compat (isolated: ${feature})"
        if [[ "${feature}" == "reverse-codegen" ]]; then
            build_reverse_models
        else
            _isolated_cargo build --config "${PROFILE_CONFIG}" --profile benchmark \
                --features "${feature}" --bin isolated_heap_profile
        fi
        "${BUILD_DIR}/isolated_heap_profile" --output "${RAW_DIR}"
    done

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
        _profile_cargo bench -p json-schema-rs-benchmark --bench cross_tool -- \
            ${criterion_args[@]+"${criterion_args[@]}"}
        for feature in ${ISOLATED_FEATURES}; do
            echo "==> in-process tier: criterion timing (isolated: ${feature})"
            _isolated_cargo bench --config "${PROFILE_CONFIG}" --profile benchmark \
                --features "${feature}" --bench isolated -- \
                ${criterion_args[@]+"${criterion_args[@]}"}
        done
    fi

    # 7. CLI timing.
    echo "==> CLI tier: hyperfine (warmup ${BENCH_CLI_WARMUP:-3}, runs ${BENCH_CLI_RUNS:-15})"
    run_cli_tier "${RAW_DIR}"

    # 8. Aggregate.
    echo "==> aggregating results"
    "${BUILD_DIR}/aggregate" \
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
