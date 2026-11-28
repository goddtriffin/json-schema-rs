#!/usr/bin/env bash
#
# run_differential.sh — top-level driver for the json-schema-rs differential-testing harness.
#
# Pipeline:
#   1. Check prerequisites (uv, a running container engine, the vendored JSON Schema Test
#      Suite) and abort up front with a hint if any is missing; fetch the latest Bowtie.
#   2. Build our Bowtie harness image (localhost/json-schema-rs:latest, labelled
#      repo=json-schema-rs so `make docker-clean` finds it) from the repo root.
#   3. Run the official test suite (validation/tests/) through Bowtie, once per draft, for our
#      harness plus every competitor implementation in COMPETITORS
#      -> reports/suite/<draft>.jsonl, plus its optional tests (validation/tests/optional/)
#      -> reports/suite/<draft>__optional.jsonl.
#   4. Run every benchmark fixture in the manifest (real-world / massive schemas) through
#      `bowtie run`, with the fixture's resource documents in each case's registry, once for its
#      valid instances and once for its invalid instances
#      -> reports/fixtures/<size>__<name>__{valid,invalid}.jsonl.
#   5. Build the summarized, committed results.{json,md} (per-draft compliance, coverage vs
#      correctness, disagreements with the majority, and the in-process error-message
#      side-by-side) with the differential_report bin.
#
# Raw Bowtie reports (reports/) are gitignored; results.{json,md} are committed.
#
# Portable across Linux, macOS (bash 3.2, BSD tools) and Windows (Git Bash or WSL): Bowtie and
# Python come from uv, timeouts are done in Python, and temporary files go under mktemp.
#
# Env knobs:
#   DIFF_DRAFTS             space-separated suite drafts to run
#                           (default: draft4 draft6 draft7 draft2019-09 draft2020-12)
#   DIFF_SKIP_IMAGE_BUILD=1 reuse an existing localhost/json-schema-rs:latest image
#   DIFF_SKIP_BOWTIE=1      reuse existing reports/ and only rebuild results.{json,md}
#   DIFF_BOWTIE_TIMEOUT     seconds before a single bowtie invocation is treated as hung
#                           (default 900)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CRATE_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
REPO_ROOT="$(cd "${CRATE_DIR}/.." && pwd)"
SUITE_DIR="${REPO_ROOT}/research/json-schema-test-suite"
FIXTURES_DIR="${REPO_ROOT}/json_schema_rs_benchmark/fixtures"
REPORTS_DIR="${CRATE_DIR}/reports"

# Bowtie, always the latest release: the repository tracks the latest Bowtie and test suite
# (research/scripts/clone-json-schema-test-suite.sh) and adapts when they change. uvx runs it
# on every platform, with no global install.
BOWTIE=(uvx --quiet --from bowtie-json-schema@latest bowtie)
# Python from uv too: `python3` is missing (or a store stub) on Windows.
PYTHON=(uv run --quiet --no-project python)

# Our harness image, its label (`make docker-clean` removes everything carrying it), and the id
# Bowtie records for it in reports.
OUR_IMAGE="localhost/json-schema-rs:latest"
OUR_LABEL="repo=json-schema-rs"
OUR_CONNECTABLE="image:localhost/json-schema-rs"
OUR_REPORT_ID="localhost/json-schema-rs"

# Competitor implementations (Bowtie connectable ids, images under ghcr.io/bowtie-json-schema).
# Chosen for spec compliance, ecosystem weight, and multi-draft support, across languages.
COMPETITORS=(
  rust-jsonschema
  rust-boon
  js-ajv
  js-hyperjump
  python-jsonschema
  java-networknt-json-schema-validator
  dotnet-jsonschema-net
  cpp-blaze
  go-jsonschema
)

DIFF_DRAFTS="${DIFF_DRAFTS:-draft4 draft6 draft7 draft2019-09 draft2020-12}"
DIFF_BOWTIE_TIMEOUT="${DIFF_BOWTIE_TIMEOUT:-900}"

echo "==> json-schema-rs differential-testing harness"

# --- 1. Prerequisites --------------------------------------------------------------------
if ! command -v uv >/dev/null 2>&1 || ! command -v uvx >/dev/null 2>&1; then
  echo "error: uv not found. Install it: https://docs.astral.sh/uv/getting-started/installation/" >&2
  exit 1
fi
if ! docker info >/dev/null 2>&1; then
  echo "error: no running container engine. Start Docker (or another engine Bowtie supports)." >&2
  exit 1
fi
if [[ ! -d "${SUITE_DIR}/validation/tests" ]]; then
  echo "error: JSON Schema Test Suite not found at ${SUITE_DIR}. Run: make vendor_test_suite" >&2
  exit 1
fi
# Fetch the latest Bowtie once up front, so the first timed run does not include the download.
echo "==> $("${BOWTIE[@]}" --version)"

# Competitors run on the REQUIRED suite only, and left out of the optional-suite and fixture
# runs. cpp-blaze's harness answers a case without a `seq` whenever a schema has a `$ref` it
# cannot resolve (optional cross-draft.json, refOfUnknownKeyword.json, format-assertion.json,
# and the package-json fixture's external refs), which crashes (alone) or hangs (alongside
# others) Bowtie's whole run.
REQUIRED_SUITE_ONLY_COMPETITORS=(cpp-blaze)

# select_implementations <dialect>: set IMPLEMENTATIONS to our harness plus every competitor that
# supports <dialect> (a Bowtie dialect short name). Competitors lacking a draft (e.g. draft 4)
# are left out of that draft's run instead of failing it; the report shows who took part.
select_implementations() {
  local filter_args=()
  for competitor in "${COMPETITORS[@]}"; do
    filter_args+=(-i "${competitor}")
  done
  IMPLEMENTATIONS=("${OUR_REPORT_ID}")
  while IFS= read -r supported; do
    supported="${supported%$'\r'}"
    [[ -n "${supported}" ]] && IMPLEMENTATIONS+=("${supported}")
  done < <("${BOWTIE[@]}" filter-implementations --supports-dialect "$1" "${filter_args[@]}")
}

# drop_required_suite_only_competitors: remove REQUIRED_SUITE_ONLY_COMPETITORS from
# IMPLEMENTATIONS (for the optional-suite and fixture runs).
drop_required_suite_only_competitors() {
  local kept=()
  local id excluded keep
  for id in "${IMPLEMENTATIONS[@]}"; do
    keep=1
    for excluded in "${REQUIRED_SUITE_ONLY_COMPETITORS[@]}"; do
      [[ "${id}" == "${excluded}" ]] && keep=0
    done
    [[ "${keep}" == 1 ]] && kept+=("${id}")
  done
  IMPLEMENTATIONS=("${kept[@]}")
}

# implementation_args: echo `-i <id>` for every id in IMPLEMENTATIONS (our harness by its
# image connectable).
implementation_args() {
  printf -- '-i %s ' "${OUR_CONNECTABLE}"
  for id in "${IMPLEMENTATIONS[@]:1}"; do
    printf -- '-i %s ' "${id}"
  done
}

# schema_dialect <schema_file>: echo the Bowtie dialect short name for the schema's `$schema`,
# defaulting to 2020-12 (our library's default) when the schema declares none.
schema_dialect() {
  local uri
  uri="$(grep -o '"\$schema"[[:space:]]*:[[:space:]]*"[^"]*"' "$1" | head -n 1 | sed 's/.*"\([^"]*\)"$/\1/' || true)"
  case "${uri}" in
    *draft-04*) echo 4 ;;
    *draft-06*) echo 6 ;;
    *draft-07*) echo 7 ;;
    *2019-09*) echo 2019-09 ;;
    *) echo 2020-12 ;;
  esac
}

# A timeout every platform has (GNU `timeout` is not on macOS): run argv[3:] with stdout to
# argv[2] and stderr to argv[2].log; past argv[1] seconds, stop it and exit 124. uv passes the
# stop on to the Bowtie process it started.
RUN_WITH_TIMEOUT_PY='
import subprocess, sys
seconds, out, command = float(sys.argv[1]), sys.argv[2], sys.argv[3:]
with open(out, "wb") as stdout, open(out + ".log", "wb") as stderr:
    process = subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=stdout, stderr=stderr)
    try:
        sys.exit(process.wait(timeout=seconds))
    except subprocess.TimeoutExpired:
        process.terminate()
        try:
            process.wait(timeout=30)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()
        sys.exit(124)
'

# run_bowtie <out_file> <bowtie args...>: run bowtie, writing its report to <out_file> and its
# (verbose) diagnostics to <out_file>.log. Bowtie exits non-zero when any implementation fails
# or errors on a test, which is expected here, so the exit status is not fatal; instead the
# report header must list every selected implementation, so one that failed to start aborts
# the run rather than silently shrinking the comparison.
run_bowtie() {
  local out="$1"
  shift
  local status=0
  "${PYTHON[@]}" -c "${RUN_WITH_TIMEOUT_PY}" "${DIFF_BOWTIE_TIMEOUT}" "${out}" "${BOWTIE[@]}" "$@" ||
    status=$?
  if [[ "${status}" == 124 ]]; then
    echo "error: bowtie timed out after ${DIFF_BOWTIE_TIMEOUT}s writing ${out}; see ${out}.log" >&2
    exit 1
  fi
  local header
  header="$(head -n 1 "${out}")"
  for id in "${IMPLEMENTATIONS[@]}"; do
    if [[ "${header}" != *"\"${id}\""* ]]; then
      echo "error: implementation ${id} is missing from ${out}; see ${out}.log" >&2
      exit 1
    fi
  done
}

if [[ "${DIFF_SKIP_BOWTIE:-0}" == "1" ]]; then
  echo "==> reusing existing Bowtie reports (DIFF_SKIP_BOWTIE=1)"
else
  # --- 2. Build our harness image ----------------------------------------------------------
  if [[ "${DIFF_SKIP_IMAGE_BUILD:-0}" == "1" ]]; then
    echo "==> reusing existing ${OUR_IMAGE} (DIFF_SKIP_IMAGE_BUILD=1)"
  else
    echo "==> building ${OUR_IMAGE}"
    docker build --quiet --label "${OUR_LABEL}" -f "${CRATE_DIR}/Dockerfile" -t "${OUR_IMAGE}" \
      "${REPO_ROOT}" >/dev/null
    # The rebuild leaves the previous image dangling; remove only this repo's dangling images.
    docker image prune -f --filter "label=${OUR_LABEL}" >/dev/null
  fi

  rm -rf "${REPORTS_DIR}"
  mkdir -p "${REPORTS_DIR}/suite" "${REPORTS_DIR}/fixtures"

  # --- 3. Official test suite, per draft ---------------------------------------------------
  # shellcheck disable=SC2046
  for draft in ${DIFF_DRAFTS}; do
    select_implementations "${draft}"
    echo "==> bowtie suite ${draft} (${#IMPLEMENTATIONS[@]} implementations)"
    run_bowtie "${REPORTS_DIR}/suite/${draft}.jsonl" \
      suite -D "${draft}" $(implementation_args) "${SUITE_DIR}/validation/tests"
    # `bowtie suite <dir>` runs only the directory's own files, so optional/ is its own run.
    drop_required_suite_only_competitors
    echo "==> bowtie suite ${draft} (optional, ${#IMPLEMENTATIONS[@]} implementations)"
    run_bowtie "${REPORTS_DIR}/suite/${draft}__optional.jsonl" \
      suite -D "${draft}" $(implementation_args) "${SUITE_DIR}/validation/tests/optional"
  done

  # --- 4. Benchmark fixtures (real-world / massive schemas) --------------------------------
  # Fixtures and their instances come from the benchmark manifest, in manifest order (the
  # report builder names test i after manifest instance i). Each (fixture, expectation) becomes
  # one `bowtie run` case whose registry holds the fixture's resource documents (resources/*.json)
  # under their root $id, so every implementation gets the documents the schema references, the
  # way the benchmark registers them. Each line is `<id>\t<schema>\t<expectation>\t<case file>`.
  cases_dir="$(mktemp -d)"
  while IFS=$'\t' read -r id schema_path expectation case_file; do
    dialect="$(schema_dialect "${FIXTURES_DIR}/${schema_path}")"
    select_implementations "${dialect}"
    drop_required_suite_only_competitors
    echo "==> bowtie run ${id} (${expectation}, dialect ${dialect})"
    # shellcheck disable=SC2046
    run_bowtie "${REPORTS_DIR}/fixtures/${id//\//__}__${expectation}.jsonl" \
      run $(implementation_args) -D "${dialect}" "${case_file}"
  done < <("${PYTHON[@]}" -c '
import json, os, sys
sys.stdout.reconfigure(newline="\n")  # no \r on Windows, which `read` would keep
fixtures_dir, cases_dir = sys.argv[1], sys.argv[2]
def load(path):
    with open(os.path.join(fixtures_dir, path)) as f:
        return json.load(f)
with open(os.path.join(fixtures_dir, "manifest.json")) as f:
    manifest = json.load(f)
for fixture in manifest:
    registry = {resource["id"]: load(resource["path"]) for resource in fixture.get("resources", [])}
    for expectation in ("valid", "invalid"):
        instances = fixture[expectation + "_instances"]
        if not instances:
            continue
        case = {
            "description": fixture["id"],
            "schema": load(fixture["schema"]),
            "tests": [
                {"description": path, "instance": load(path), "valid": expectation == "valid"}
                for path in instances
            ],
        }
        if registry:
            case["registry"] = registry
        case_file = os.path.join(cases_dir, fixture["id"].replace("/", "__") + "__" + expectation + ".json")
        with open(case_file, "w") as f:
            json.dump(case, f)
        print("\t".join([fixture["id"], fixture["schema"], expectation, case_file]))
' "${FIXTURES_DIR}" "${cases_dir}")
  rm -rf "${cases_dir}"
fi

# --- 5. Summarize into committed results -------------------------------------------------
echo "==> building results.{json,md}"
cargo run --quiet --release --package json-schema-rs-differential --bin differential_report -- \
  --reports-dir "${REPORTS_DIR}" \
  --suite-dir "${SUITE_DIR}" \
  --fixtures-dir "${FIXTURES_DIR}" \
  --ours "${OUR_REPORT_ID}" \
  --out-dir "${CRATE_DIR}"

echo "==> done: ${CRATE_DIR}/results.md"
