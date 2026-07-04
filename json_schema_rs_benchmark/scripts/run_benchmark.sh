#!/usr/bin/env bash
#
# run_benchmark.sh — top-level driver for the json-schema-rs benchmark harness.
#
# Pipeline:
#   1. Build the benchmark crate and the `jsonschemars` CLI (release).
#   2. Run our in-process criterion micro-benchmarks (regression gating for our lib).
#   3. For each fixture, time every wired tool end-to-end and capture peak RSS; record the
#      codegen compat matrix (pass/fail per tool/fixture).
#   4. Aggregate all raw captures into committed results.json + results.md.
#
# Measurement: every CLI run is wrapped in `/usr/bin/time` (macOS `-l` / Linux `-v`), which
# yields BOTH wall-time (`real`) and peak resident set size in one shot; the min of
# `${BENCH_REPEATS}` timed runs is taken as the wall-time. Precise in-process, startup-free
# numbers for our own lib come from criterion (step 2).
#
# Required tools: every competitor CLI the harness benchmarks (see required_external_tools in
# scripts/tools.sh) must be installed. The driver aborts up front with install hints if any is
# missing, rather than silently omitting a competitor and misrepresenting the comparison.
# Fixture-level incompatibility is different: a tool that cannot handle a specific schema is
# recorded as a compat failure, never a hard error.
#
# Env knobs:
#   BENCH_REPEATS           timed repeats per CLI measurement (default 5)
#   BENCH_CRITERION_ARGS    extra args passed to `cargo bench` (default bounds runtime)
#   BENCH_SKIP_CRITERION=1  skip the criterion step (faster iteration)
#
# OWNER: foundation/integration. Per-competitor invocation definitions live in
# scripts/tools.sh; fixtures live in ../fixtures.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CRATE_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
REPO_ROOT="$(cd "${CRATE_DIR}/.." && pwd)"
FIXTURES_DIR="${CRATE_DIR}/fixtures"
RAW_DIR="${CRATE_DIR}/target/benchmark-raw"
CRITERION_DIR="${REPO_ROOT}/target/criterion"
export REPO_ROOT

BENCH_REPEATS="${BENCH_REPEATS:-5}"

echo "==> json-schema-rs benchmark harness"
echo "    crate:    ${CRATE_DIR}"
echo "    fixtures: ${FIXTURES_DIR}"

# --- 1. Build ---------------------------------------------------------------------------
echo "==> building release binaries"
cargo build --release --package json-schema-rs --bin jsonschemars
cargo build --release --package json-schema-rs-benchmark

# --- 2. Our in-process criterion hot paths ----------------------------------------------
if [[ "${BENCH_SKIP_CRITERION:-0}" == "1" ]]; then
  echo "==> skipping criterion (BENCH_SKIP_CRITERION=1)"
else
  echo "==> running criterion hot-path benchmarks (our lib)"
  # Bounded defaults so `make benchmark` stays practical; override with BENCH_CRITERION_ARGS.
  # shellcheck disable=SC2086
  cargo bench --package json-schema-rs-benchmark --bench hot_paths -- \
    ${BENCH_CRITERION_ARGS:---warm-up-time 1 --measurement-time 3 --sample-size 30}
fi

# --- 3. Cross-tool CLI timing + memory + compat -----------------------------------------
rm -rf "${RAW_DIR}"
mkdir -p "${RAW_DIR}"
CLI_TSV="${RAW_DIR}/cli_measurements.tsv"
COMPAT_TSV="${RAW_DIR}/compat.tsv"
: > "${CLI_TSV}"
: > "${COMPAT_TSV}"

# Tool definitions (competitor-wiring leaf). Provides: codegen_tools, validation_tools,
# run_codegen <tool> <schema> <out_dir>, run_validate <tool> <schema> <payload>.
TOOLS_SH="${SCRIPT_DIR}/tools.sh"
if [[ -f "${TOOLS_SH}" ]]; then
  # shellcheck source=/dev/null
  source "${TOOLS_SH}"

  # Every competitor must be installed, or the run would silently omit tools and misrepresent
  # the comparison. Abort with install hints rather than produce a partial, misleading result.
  echo "==> checking required competitor tools"
  if ! check_required_tools; then
    echo "!! required competitor tools are missing (see above); aborting." >&2
    echo "   install the tools listed above, or edit required_external_tools in" >&2
    echo "   scripts/tools.sh if you intend to benchmark a subset." >&2
    exit 1
  fi
else
  echo "!! scripts/tools.sh not found; skipping cross-tool measurements" >&2
fi

# Pick the wall-time / RSS parser for this OS.
case "$(uname)" in
  Darwin) TIME_FLAG="-l" ;;
  *)      TIME_FLAG="-v" ;;
esac

# measure_run <argv...> : times the given command (a tools.sh shell function + args)
# BENCH_REPEATS times under /usr/bin/time, exporting MEASURE_RC (last exit code),
# MEASURE_WALL (min real seconds) and MEASURE_RSS (max peak bytes). The command runs in a
# child `bash -c` that first sources tools.sh (so every helper is defined), because
# /usr/bin/time cannot exec a shell function directly.
measure_run() {
  local tf best_wall="" max_rss=0 rc=0 i wall rss
  tf="$(mktemp)"
  for (( i = 0; i < BENCH_REPEATS; i++ )); do
    if /usr/bin/time "${TIME_FLAG}" \
      bash -c 'source "$1"; shift; "$@"' _ "${TOOLS_SH}" "$@" >/dev/null 2>"${tf}"; then
      rc=0
    else
      rc=$?
    fi
    if [[ "${TIME_FLAG}" == "-l" ]]; then
      wall="$(awk '{for(j=1;j<=NF;j++) if($j=="real") print $(j-1)}' "${tf}" | head -1)"
      rss="$(awk '/maximum resident set size/{print $1; exit}' "${tf}")"
    else
      wall="$(awk -F'[ :]' '/Elapsed .wall clock./{print $(NF-1)*60 + $NF}' "${tf}" | head -1)"
      rss="$(awk '/Maximum resident set size/{print $NF*1024; exit}' "${tf}")"
    fi
    [[ -z "${wall}" ]] && wall=0
    [[ -z "${rss}" ]] && rss=0
    if [[ -z "${best_wall}" ]] || awk "BEGIN{exit !(${wall} < ${best_wall})}"; then
      best_wall="${wall}"
    fi
    if (( rss > max_rss )); then max_rss="${rss}"; fi
  done
  rm -f "${tf}"
  MEASURE_RC="${rc}"
  MEASURE_WALL="${best_wall:-0}"
  MEASURE_RSS="${max_rss}"
}

emit_measure() { printf '%s\t%s\t%s\t%s\n' "$1" "$2" "$3" "$4" >> "${CLI_TSV}"; }
emit_compat()  { printf '%s\t%s\t%s\t%s\n' "$1" "$2" "$3" "$4" >> "${COMPAT_TSV}"; }

first_json() { find "$1" -type f -name '*.json' 2>/dev/null | sort | head -1; }

if declare -F codegen_tools >/dev/null && [[ -d "${FIXTURES_DIR}" ]]; then
  while IFS= read -r schema; do
    [[ -z "${schema}" ]] && continue
    fdir="$(dirname "${schema}")"
    fid="${fdir#"${FIXTURES_DIR}/"}"
    echo "==> fixture ${fid}"

    # ---- codegen axis ----
    for tool in $(codegen_tools); do
      out="$(mktemp -d)"
      if measure_run run_codegen "${tool}" "${schema}" "${out}" && [[ "${MEASURE_RC}" == "0" ]]; then
        emit_compat "${tool}" "${fid}" "pass" ""
        emit_measure "${tool}" "${fid}" "codegen_wall_time_secs" "${MEASURE_WALL}"
        emit_measure "${tool}" "${fid}" "peak_memory_bytes" "${MEASURE_RSS}"
      else
        emit_compat "${tool}" "${fid}" "fail" "codegen exited ${MEASURE_RC}"
      fi
      rm -rf "${out}"
    done

    # ---- validation axis ----
    valid_payload="$(first_json "${fdir}/instances/valid")"
    invalid_payload="$(first_json "${fdir}/instances/invalid")"
    for tool in $(validation_tools); do
      if [[ -n "${valid_payload}" ]]; then
        measure_run run_validate "${tool}" "${schema}" "${valid_payload}"
        emit_measure "${tool}" "${fid}" "validate_valid_time_secs" "${MEASURE_WALL}"
        emit_measure "${tool}" "${fid}" "peak_memory_bytes" "${MEASURE_RSS}"
      fi
      if [[ -n "${invalid_payload}" ]]; then
        measure_run run_validate "${tool}" "${schema}" "${invalid_payload}"
        emit_measure "${tool}" "${fid}" "validate_invalid_time_secs" "${MEASURE_WALL}"
      fi
    done
  done < <(find "${FIXTURES_DIR}" -type f -name 'schema.json' 2>/dev/null | sort)
else
  echo "!! no tool definitions or no fixtures; only criterion (our-lib) results will be reported" >&2
fi

# --- 4. Aggregate -----------------------------------------------------------------------
echo "==> aggregating results"
"${REPO_ROOT}/target/release/aggregate" \
  --input "${RAW_DIR}" \
  --criterion "${CRITERION_DIR}" \
  --output "${CRATE_DIR}"

echo "==> done. results written to:"
echo "    ${CRATE_DIR}/results.json"
echo "    ${CRATE_DIR}/results.md"
