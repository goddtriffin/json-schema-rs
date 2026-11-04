#!/usr/bin/env bash
#
# tools.sh — per-tool CLI invocation definitions for the benchmark harness's CLI tier.
#
# This file is a library: scripts/run_benchmark.sh `source`s it. It does not run standalone and
# has no `set -e` or top-level side effects. The sourcing script must set `REPO_ROOT` (repo root);
# our own release binaries live at "$REPO_ROOT/target/release/{jsonschemars,boon_validate}", and the
# non-Rust competitors in environments installed from the lock files under
# "$REPO_ROOT/json_schema_rs_benchmark/competitors/" (node: npm, python: uv; see
# install_competitor_envs).
# Must stay compatible with bash 3.2 (the macOS system bash).
#
# The harness has two tiers:
#   - In-process tier: every tool linked as a Rust library and timed by criterion, with peak heap
#     from a counting allocator (benches/cross_tool.rs, src/bin/heap_profile.rs). Not defined here.
#   - CLI tier: every tool's command-line front end timed end to end (process startup, file I/O,
#     parsing, compile, generate/validate, exit) by hyperfine. THIS file defines how each tool's
#     CLI is invoked.
#
# ---------------------------------------------------------------------------------------
# API (run_benchmark.sh depends on these names and argument orders):
#
#   required_external_tools          -> external binaries that must be on PATH (one per line)
#   _install_hint <cmd>              -> install command for a required tool
#   check_required_tools             -> 0 if all present; else prints each missing one, returns 1
#   tool_version <cmd>               -> version number of an external tool (e.g. 1.20.0)
#   install_competitor_envs          -> install the Node and Python competitors from their lock files
#   npm_locked_version <package>     -> version package-lock.json pins for an npm competitor
#   pypi_locked_version <package>    -> version uv.lock pins for a Python competitor
#   tool_runtime <id>                -> language and runtime a tool runs on (for the Tools table)
#   codegen_tools                    -> space-separated codegen tool ids
#   validation_tools                 -> space-separated validation tool ids
#   loads_resources <tool> <operation>       -> 0 if the tool's CLI can take extra documents
#   codegen_cmd  <tool> <schema> <out_dir> [resource...]  -> ECHO the codegen command line
#   validate_cmd <tool> <schema> <payload> [resource...]  -> ECHO the validation command line
#   run_cmd <command_line>           -> execute a command line produced by the two functions above
#   run_cmd_offline <command_line>   -> same, with network access denied where supported
#   offline_supported                -> true when run_cmd_offline can deny network access
#   tool_display_name <id>           -> human-friendly label (echoes the id if unknown)
#
# Command lines are single strings of shell-quoted words (`printf %q`) with no shell syntax, so
# they can be handed to `hyperfine -N` (which splits words itself and runs the program with no
# intermediate shell) and also run verbatim via `run_cmd` for the correctness probe.
#
# Exit-code convention for validation: 0 = instance valid; non-zero = invalid (or error).
#
# Resources: a fixture may vendor the documents its schema references (fixtures/<size>/<name>/
# resources/*.json, each with an absolute root `$id`). They are passed to a tool's CLI only where
# that CLI can take extra documents (see loads_resources); every other cell on such a fixture is a
# compatibility failure, never a run without the documents.
#
# ---------------------------------------------------------------------------------------
# TOOL CLI SYNTAXES
#
#   jsonschemars (our CLI, "$REPO_ROOT/target/release/jsonschemars"):
#       codegen:  jsonschemars generate rust -o <out_dir> <schema> [resource...]
#                 (every input file is reachable from the others by its file URI and root $id;
#                 each also gets its own output file)
#       validate: jsonschemars validate -s <schema> [-r <resource>]... -p <payload>
#                 (each -r registers a document under its file URI and root $id)
#
#   typify (https://github.com/oxidecomputer/typify, `cargo install cargo-typify`):
#       cargo-typify typify <schema> --output <out_dir>/<base>.rs   (no extra documents)
#     The `cargo-typify` binary is invoked DIRECTLY. Going through `cargo typify` would add the
#     cargo wrapper's own startup to every measurement. When run directly, cargo subcommands take
#     their subcommand name as the first argument, hence the repeated `typify`.
#
#   jsonschema-cli (https://github.com/Stranger6667/jsonschema, `cargo install jsonschema-cli`):
#       jsonschema-cli validate <schema> -i <payload>   (no extra documents)
#
#   boon (our thin wrapper bin over the boon crate, "$REPO_ROOT/target/release/boon_validate";
#         boon's own `boon-cli` loads documents by file URL only, so it cannot take a fixture's
#         resources under their $id):
#       boon_validate -s <schema> [-r <resource>]... -p <payload>
#     (each -r registers a document under its root $id, as boon's Compiler::add_resource does
#     in-process)
#
#   blaze (https://github.com/sourcemeta/blaze, C++; through the sourcemeta/jsonschema CLI, npm
#         package @sourcemeta/jsonschema, whose native binary is run directly, not its Node shim):
#       jsonschema validate <schema> <payload> [--resolve <resource>]...
#     (exhaustive mode, the CLI's default; each --resolve registers a document under its $id)
#
#   ajv (https://github.com/ajv-validator/ajv, JavaScript; our thin wrapper
#        competitors/node/ajv_validate.mjs, since ajv-cli is unmaintained):
#       node ajv_validate.mjs -s <schema> [-r <resource>]... -p <payload>
#
#   python-jsonschema (https://github.com/python-jsonschema/jsonschema; our thin wrapper
#        competitors/python/jsonschema_validate.py, since its own CLI is deprecated):
#       python jsonschema_validate.py -s <schema> [-r <resource>]... -p <payload>
#
#   fastjsonschema (https://github.com/horejsek/python-fastjsonschema; our thin wrapper
#        competitors/python/fastjsonschema_validate.py, since it ships no validation command):
#       python fastjsonschema_validate.py -s <schema> [-r <resource>]... -p <payload>
#
#   quicktype (https://github.com/glideapps/quicktype, TypeScript on Node.js; npm package):
#       quicktype --src-lang schema --lang rust --out <out_dir>/<base>.rs <schema>
#     (no extra documents: quicktype fetches remote $refs itself)
#
#   datamodel-code-generator (https://github.com/koxudaxi/datamodel-code-generator, Python; emits
#        pydantic models):
#       datamodel-codegen --input <schema> --input-file-type jsonschema
#           --output-model-type pydantic_v2.BaseModel --formatters builtin --output <out_dir>/<base>.py
#     (--formatters builtin is the tool's announced next default and what its deprecation warning
#     recommends; today's default shells out to black and isort. No extra documents: it resolves
#     $refs by file path or URL.)
#
#   json-schema-to-typescript (https://github.com/bcherny/json-schema-to-typescript, TypeScript on
#        Node.js; emits TypeScript declarations, formatted with Prettier by default):
#       json2ts -i <schema> -o <out_dir>/<base>.d.ts   (no extra documents)
#
# schemafy is not on the CLI tier: it is a proc-macro/library crate with no command-line tool.
# ---------------------------------------------------------------------------------------

# --- helpers ---------------------------------------------------------------------------

_jsonschemars_bin() { printf '%s/target/release/jsonschemars' "${REPO_ROOT:?REPO_ROOT not set}"; }
_boon_bin() { printf '%s/target/release/boon_validate' "${REPO_ROOT:?REPO_ROOT not set}"; }
_competitors_dir() { printf '%s/json_schema_rs_benchmark/competitors' "${REPO_ROOT:?REPO_ROOT not set}"; }
_node_dir() { printf '%s/node' "$(_competitors_dir)"; }
_python_dir() { printf '%s/python' "$(_competitors_dir)"; }
_python_bin() { printf '%s/.venv/bin/python' "$(_python_dir)"; }

# _blaze_bin: the sourcemeta/jsonschema native binary the npm package ships for this machine.
_blaze_bin() {
    local os arch
    case "$(uname -s)" in
        Darwin) os=darwin ;;
        Linux)  os=linux ;;
        *)      os=unsupported ;;
    esac
    case "$(uname -m)" in
        arm64 | aarch64) arch=arm64 ;;
        x86_64 | amd64)  arch=x86_64 ;;
        *)               arch=unsupported ;;
    esac
    printf '%s/node_modules/@sourcemeta/jsonschema/build/github-releases/jsonschema-%s-%s' \
        "$(_node_dir)" "${os}" "${arch}"
}

# _has <cmd>: true if <cmd> is on PATH (silent).
_has() { command -v "$1" >/dev/null 2>&1; }

# _quote_argv <word>...: echo the words as one shell-quoted command line.
_quote_argv() {
    local out="" word
    for word in "$@"; do
        out="${out:+${out} }$(printf '%q' "${word}")"
    done
    printf '%s\n' "${out}"
}

# --- required tools --------------------------------------------------------------------

# required_external_tools: every external binary the harness needs. A missing competitor would
# silently drop out of the comparison and misrepresent it, so the driver aborts instead. Our own
# bins (jsonschemars, boon_validate) are built by the driver and are not listed.
required_external_tools() {
    printf '%s\n' hyperfine cargo-typify jsonschema-cli node npm uv
}

# _install_hint <cmd>: how to install a required tool.
_install_hint() {
    case "$1" in
        hyperfine)      echo "cargo install hyperfine" ;;
        cargo-typify)   echo "cargo install cargo-typify" ;;
        jsonschema-cli) echo "cargo install jsonschema-cli" ;;
        node | npm)     echo "brew install node (or https://nodejs.org)" ;;
        uv)             echo "brew install uv (or https://docs.astral.sh/uv)" ;;
        *)              echo "(unknown install method)" ;;
    esac
}

# check_required_tools: 0 if every required tool is on PATH; otherwise prints each missing tool
# with its install command and returns 1.
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

# tool_version <cmd>: echo the first dotted version number the tool reports (or "unknown").
tool_version() {
    local raw
    case "$1" in
        # As a cargo subcommand binary, cargo-typify expects its subcommand name first.
        cargo-typify) raw="$(cargo-typify typify --version 2>&1 || true)" ;;
        *)            raw="$("$1" --version 2>&1 || true)" ;;
    esac
    printf '%s\n' "${raw}" | awk '
        { for (i = 1; i <= NF; i++) if ($i ~ /^v?[0-9]+\.[0-9]+/) { sub(/^v/, "", $i); print $i; found = 1; exit } }
        END { if (!found) print "unknown" }'
}

# --- non-Rust competitor environments ---------------------------------------------------

# install_competitor_envs: install the Node and Python competitors exactly as their committed lock
# files pin them (competitors/node/package-lock.json, competitors/python/uv.lock), into
# git-ignored directories next to them. uv also provides the pinned Python (.python-version).
install_competitor_envs() {
    npm ci --prefix "$(_node_dir)" --no-audit --no-fund --loglevel=error >/dev/null &&
        uv sync --frozen --quiet --project "$(_python_dir)"
}

# npm_locked_version <package>: the version competitors/node/package-lock.json pins ("unknown" if
# absent).
npm_locked_version() {
    node -e '
        const lock = require(process.argv[1]);
        const entry = lock.packages["node_modules/" + process.argv[2]];
        console.log(entry ? entry.version : "unknown");' \
        "$(_node_dir)/package-lock.json" "$1"
}

# pypi_locked_version <package>: the version competitors/python/uv.lock pins ("unknown" if absent).
pypi_locked_version() {
    awk -v name="$1" '
        $0 == "name = \"" name "\"" { getline; gsub(/version = |"/, ""); print; found = 1; exit }
        END { if (!found) print "unknown" }' "$(_python_dir)/uv.lock"
}

# tool_runtime <id>: the language and runtime a tool runs on, shown next to its version so readers
# can tell a runtime's fixed cost (see the startup baseline) from the work.
tool_runtime() {
    case "$1" in
        jsonschemars | jsonschema | jsonschema-cli | boon | typify)
            echo "Rust (native)" ;;
        blaze)
            echo "C++ (native)" ;;
        ajv)
            echo "JavaScript (Node.js $(tool_version node))" ;;
        quicktype | json-schema-to-typescript)
            echo "TypeScript (Node.js $(tool_version node))" ;;
        python-jsonschema | fastjsonschema | datamodel-code-generator)
            echo "Python (CPython $(tool_version "$(_python_bin)"))" ;;
        hyperfine)
            echo "Rust (native); times the CLI tier" ;;
        rustc)
            echo "compiles every Rust tool (release profile)" ;;
        *)
            echo "unknown" ;;
    esac
}

# --- tool sets -------------------------------------------------------------------------

codegen_tools() {
    printf '%s\n' "jsonschemars typify quicktype datamodel-code-generator json-schema-to-typescript"
}
validation_tools() {
    printf '%s\n' "jsonschemars jsonschema-cli boon blaze ajv python-jsonschema fastjsonschema"
}

# --- command lines ---------------------------------------------------------------------

# loads_resources <tool> <operation>: true when <tool>'s CLI can take a fixture's resource
# documents for <operation> (codegen, validate_valid, validate_invalid).
loads_resources() {
    case "$1:$2" in
        jsonschemars:*)         return 0 ;;
        boon:validate_*)        return 0 ;;
        blaze:validate_*)       return 0 ;;
        ajv:validate_*)         return 0 ;;
        python-jsonschema:validate_*) return 0 ;;
        fastjsonschema:validate_*)    return 0 ;;
        *)                      return 1 ;;
    esac
}

# _no_resources <tool> <operation> <resource count>: fail (status 3) when resources were given
# to a tool whose CLI cannot take them (callers check loads_resources first).
_no_resources() {
    if [[ "$3" -gt 0 ]]; then
        echo "$1 cannot take resource documents for $2" >&2
        return 3
    fi
}

# codegen_cmd <tool> <schema> <out_dir> [resource...]: echo the command line generating Rust for
# <schema> into <out_dir> (which must already exist), with the resource documents the schema
# references. Returns 2 for an unknown tool, 3 for resources the tool cannot take.
codegen_cmd() {
    local tool="$1" schema="$2" out_dir="$3" base
    shift 3
    case "${tool}" in
        jsonschemars)
            _quote_argv "$(_jsonschemars_bin)" generate rust -o "${out_dir}" "${schema}" ${1+"$@"}
            ;;
        typify)
            _no_resources "${tool}" codegen "$#" || return
            base="$(basename "${schema}")"
            base="${base%.*}"
            _quote_argv cargo-typify typify "${schema}" --output "${out_dir}/${base}.rs"
            ;;
        quicktype)
            _no_resources "${tool}" codegen "$#" || return
            base="$(basename "${schema}")"
            base="${base%.*}"
            _quote_argv "$(_node_dir)/node_modules/.bin/quicktype" --src-lang schema --lang rust \
                --out "${out_dir}/${base}.rs" "${schema}"
            ;;
        datamodel-code-generator)
            _no_resources "${tool}" codegen "$#" || return
            base="$(basename "${schema}")"
            base="${base%.*}"
            _quote_argv "$(_python_dir)/.venv/bin/datamodel-codegen" --input "${schema}" \
                --input-file-type jsonschema --output-model-type pydantic_v2.BaseModel \
                --formatters builtin --output "${out_dir}/${base}.py"
            ;;
        json-schema-to-typescript)
            _no_resources "${tool}" codegen "$#" || return
            base="$(basename "${schema}")"
            base="${base%.*}"
            _quote_argv "$(_node_dir)/node_modules/.bin/json2ts" -i "${schema}" \
                -o "${out_dir}/${base}.d.ts"
            ;;
        *)
            echo "codegen_cmd: unknown codegen tool '${tool}'" >&2
            return 2
            ;;
    esac
}

# validate_cmd <tool> <schema> <payload> [resource...]: echo the command line validating
# <payload> against <schema> (exit 0 = valid), with the resource documents the schema references.
# Returns 2 for an unknown tool, 3 for resources the tool cannot take.
validate_cmd() {
    local tool="$1" schema="$2" payload="$3" resource script
    local -a resource_args
    shift 3
    case "${tool}" in
        jsonschemars)
            resource_args=()
            for resource in ${1+"$@"}; do
                resource_args+=(-r "${resource}")
            done
            _quote_argv "$(_jsonschemars_bin)" validate -s "${schema}" ${resource_args[@]+"${resource_args[@]}"} -p "${payload}"
            ;;
        jsonschema-cli)
            _no_resources "${tool}" validate "$#" || return
            _quote_argv jsonschema-cli validate "${schema}" -i "${payload}"
            ;;
        boon)
            resource_args=()
            for resource in ${1+"$@"}; do
                resource_args+=(-r "${resource}")
            done
            _quote_argv "$(_boon_bin)" -s "${schema}" ${resource_args[@]+"${resource_args[@]}"} -p "${payload}"
            ;;
        blaze)
            resource_args=()
            for resource in ${1+"$@"}; do
                resource_args+=(--resolve "${resource}")
            done
            _quote_argv "$(_blaze_bin)" validate "${schema}" "${payload}" ${resource_args[@]+"${resource_args[@]}"}
            ;;
        ajv)
            resource_args=()
            for resource in ${1+"$@"}; do
                resource_args+=(-r "${resource}")
            done
            _quote_argv node "$(_node_dir)/ajv_validate.mjs" -s "${schema}" ${resource_args[@]+"${resource_args[@]}"} -p "${payload}"
            ;;
        python-jsonschema | fastjsonschema)
            resource_args=()
            for resource in ${1+"$@"}; do
                resource_args+=(-r "${resource}")
            done
            if [[ "${tool}" == "python-jsonschema" ]]; then
                script=jsonschema_validate.py
            else
                script=fastjsonschema_validate.py
            fi
            _quote_argv "$(_python_bin)" "$(_python_dir)/${script}" -s "${schema}" ${resource_args[@]+"${resource_args[@]}"} -p "${payload}"
            ;;
        *)
            echo "validate_cmd: unknown validation tool '${tool}'" >&2
            return 2
            ;;
    esac
}

# run_cmd <command_line>: execute a command line from codegen_cmd / validate_cmd and return its
# exit status. Safe to eval: every word was quoted with `printf %q` and it contains no shell
# syntax.
run_cmd() {
    eval "$1"
}

# Sandbox profile denying all network access (macOS sandbox-exec).
_NO_NETWORK_PROFILE='(version 1)(allow default)(deny network*)'

# _offline_wrapper: echo which working mechanism can deny network access to a command
# (`sandbox-exec` or `unshare`), or nothing. Each candidate is tried on `true`, because a wrapper
# can be installed yet unusable (e.g. `unshare` without user namespaces).
_offline_wrapper() {
    if _has sandbox-exec && sandbox-exec -p "${_NO_NETWORK_PROFILE}" true 2>/dev/null; then
        echo sandbox-exec
    elif _has unshare && unshare -rn true 2>/dev/null; then
        echo unshare
    fi
}

# offline_supported: true when this machine can run a command with network access denied.
offline_supported() {
    [[ -n "$(_offline_wrapper)" ]]
}

# run_cmd_offline <command_line>: like run_cmd, but with network access denied when the machine
# supports it (see offline_supported), so a tool that needs the network fails instead of
# downloading. Used for the correctness probe only: the sandbox wrapper costs several
# milliseconds per process, so timed runs are not wrapped.
run_cmd_offline() {
    case "${BENCH_OFFLINE_WRAPPER-$(_offline_wrapper)}" in
        sandbox-exec) eval "sandbox-exec -p $(printf '%q' "${_NO_NETWORK_PROFILE}") $1" ;;
        unshare)      eval "unshare -rn $1" ;;
        *)            eval "$1" ;;
    esac
}

# --- display names ---------------------------------------------------------------------

tool_display_name() {
    case "$1" in
        jsonschemars)   echo "json-schema-rs" ;;
        typify)         echo "typify" ;;
        jsonschema-cli) echo "jsonschema (Stranger6667)" ;;
        jsonschema)     echo "jsonschema (Stranger6667)" ;;
        boon)           echo "boon" ;;
        blaze)          echo "Blaze (sourcemeta/jsonschema CLI)" ;;
        ajv)            echo "ajv" ;;
        python-jsonschema) echo "jsonschema (Python)" ;;
        fastjsonschema) echo "fastjsonschema" ;;
        quicktype)      echo "quicktype" ;;
        datamodel-code-generator) echo "datamodel-code-generator" ;;
        json-schema-to-typescript) echo "json-schema-to-typescript" ;;
        *)              echo "$1" ;;
    esac
}
