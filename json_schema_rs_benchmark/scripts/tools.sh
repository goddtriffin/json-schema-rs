#!/usr/bin/env bash
#
# tools.sh — per-tool CLI invocation definitions for the benchmark harness's CLI tier.
#
# This file is a library: scripts/run_benchmark.sh `source`s it. It does not run standalone and
# has no `set -e` or top-level side effects. The sourcing script must set `REPO_ROOT` (repo root).
# Every Rust tool is built with the benchmark profile (json_schema_rs_benchmark/profile.toml): our
# own binaries live at "$REPO_ROOT/target/benchmark/{jsonschemars,boon_validate,reverse_schema}",
# the Rust competitor CLIs at "$REPO_ROOT/target/benchmark-tools/bin/" (see
# install_rust_cli_competitors), and the non-Rust competitors in environments installed from the
# lock files under
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
#   profile_summary                  -> the benchmark profile's settings (e.g. codegen-units = 1, ...)
#   install_rust_cli_competitors <jsonschema version> <typify version>
#                                    -> install jsonschema-cli and cargo-typify with the benchmark profile
#   install_competitor_envs          -> install the Node and Python competitors from their lock files
#   npm_locked_version <package>     -> version package-lock.json pins for an npm competitor
#   pypi_locked_version <package>    -> version uv.lock pins for a Python competitor
#   tool_runtime <id>                -> language and runtime a tool runs on (for the Tools table)
#   codegen_tools                    -> space-separated codegen tool ids
#   validation_tools                 -> space-separated validation tool ids
#   reverse_codegen_tools            -> space-separated reverse-codegen tool ids
#   loads_resources <tool> <operation>       -> 0 if the tool's CLI can take extra documents
#   codegen_cmd  <tool> <schema> <out_dir> [resource...]  -> ECHO the codegen command line
#   validate_cmd <tool> <schema> <payload> [resource...]  -> ECHO the validation command line
#   reverse_model_dir <tool> <fixture id>    -> directory a tool's generated models go to
#   reverse_model_cmd <tool> <schema> <model_dir> [resource...]
#                                    -> ECHO the (untimed) model generation command line, or nothing
#                                       when the tool's models are compiled into reverse_schema
#   reverse_model_check <tool> <model_dir>   -> 0 if the generated models are usable (define the
#                                       root model; TypeScript type-checks), else print why
#   reverse_codegen_cmd <tool> <fixture id> <model_dir>   -> ECHO the reverse-codegen command line
#   run_cmd <command_line>           -> execute a command line produced by the functions above
#   run_cmd_offline <command_line>   -> same, with network access denied where supported
#   offline_supported                -> true when run_cmd_offline can deny network access
#   tool_display_name <id>           -> human-friendly label (echoes the id if unknown)
#   build_models <manifest> <features> <exclude file> <log dir> [cargo args...]
#                                    -> build isolated/'s model features, excluding each model
#                                       that does not compile (quick_benchmark.sh uses it too)
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
#   jsonschemars (our CLI, "$REPO_ROOT/target/benchmark/jsonschemars"):
#       codegen:  jsonschemars generate rust -o <out_dir> <schema> [resource...]
#                 (every input file is reachable from the others by its file URI and root $id;
#                 each also gets its own output file)
#       validate: jsonschemars validate -s <schema> [-r <resource>]... -p <payload>
#                 (each -r registers a document under its file URI and root $id)
#
#   typify (https://github.com/oxidecomputer/typify; the cargo-typify crate, installed by the driver):
#       cargo-typify typify <schema> --output <out_dir>/<base>.rs   (no extra documents)
#     The `cargo-typify` binary is invoked DIRECTLY. Going through `cargo typify` would add the
#     cargo wrapper's own startup to every measurement. When run directly, cargo subcommands take
#     their subcommand name as the first argument, hence the repeated `typify`.
#
#   jsonschema-cli (https://github.com/Stranger6667/jsonschema; installed by the driver):
#       jsonschema-cli validate <schema> -i <payload>   (no extra documents)
#
#   boon (our thin wrapper bin over the boon crate, "$REPO_ROOT/target/benchmark/boon_validate";
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
#
# REVERSE CODEGEN (language-native models -> JSON Schema). Each tool's models are generated from
# the fixture's schema before timing (never timed, never committed); the timed command turns the
# fixture's root model into a JSON Schema on stdout. The root model is always named `Root`.
#
#   jsonschemars, schemars (Rust; our thin wrapper bin "$REPO_ROOT/target/benchmark/reverse_schema",
#        built from isolated/ with the `reverse-codegen` feature; our codegen emits each fixture's
#        Rust types with both derives, compiled into the binary, so no model files):
#       reverse_schema <jsonschemars|schemars> <fixture id>
#     (prints `T::json_schema()` / `schemars::schema_for!(T)`; exits 1 with the reason on stderr
#     when the tool has no model for the fixture)
#
#   pydantic (https://github.com/pydantic/pydantic, Python with a Rust core; models generated by
#        datamodel-code-generator; our thin wrapper competitors/python/pydantic_schema.py, since
#        pydantic has no command line):
#       models: datamodel-codegen --input <schema> --input-file-type jsonschema
#           --output-model-type pydantic_v2.BaseModel --formatters builtin --class-name Root
#           --disable-timestamp --output <model_dir>/models.py   (no extra documents)
#       timed:  python pydantic_schema.py <model_dir>/models.py Root
#
#   zod (https://github.com/colinhacks/zod v4, TypeScript; schemas generated by quicktype; our thin
#        wrapper competitors/node/zod_schema.mjs, since zod has no command line):
#       models: quicktype --src-lang schema --lang typescript-zod --top-level Root
#           --out <model_dir>/models.ts <schema> [-S <resource>]...
#       timed:  node zod_schema.mjs <model_dir>/models.ts RootSchema
#
#   ts-json-schema-generator (https://github.com/vega/ts-json-schema-generator, TypeScript on
#        Node.js; types generated by quicktype; its own CLI, default options):
#       models: quicktype --src-lang schema --lang typescript --just-types --explicit-unions
#           --top-level Root --out <model_dir>/types.ts <schema> [-S <resource>]...
#     (--explicit-unions names every union, the root's included: without it a root that is a
#     union, such as prettierrc's object-or-string, gets no named type to pass to --type)
#       timed:  ts-json-schema-generator --path <model_dir>/types.ts --type Root
#
#   quicktype's -S (--additional-schema) registers each resource under its root $id; it takes
#   every following word, so it goes after the schema. datamodel-code-generator has no such
#   option (it resolves $refs by file path or URL).
# ---------------------------------------------------------------------------------------

# --- helpers ---------------------------------------------------------------------------

_jsonschemars_bin() { printf '%s/target/benchmark/jsonschemars' "${REPO_ROOT:?REPO_ROOT not set}"; }
_boon_bin() { printf '%s/target/benchmark/boon_validate' "${REPO_ROOT:?REPO_ROOT not set}"; }
_reverse_schema_bin() { printf '%s/target/benchmark/reverse_schema' "${REPO_ROOT:?REPO_ROOT not set}"; }
# The build profile of every Rust tool, and where the driver installs the Rust competitor CLIs.
_profile_config() { printf '%s/json_schema_rs_benchmark/profile.toml' "${REPO_ROOT:?REPO_ROOT not set}"; }
_rust_tools_root() { printf '%s/target/benchmark-tools' "${REPO_ROOT:?REPO_ROOT not set}"; }
_typify_bin() { printf '%s/bin/cargo-typify' "$(_rust_tools_root)"; }
_jsonschema_cli_bin() { printf '%s/bin/jsonschema-cli' "$(_rust_tools_root)"; }
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
# bins (jsonschemars, boon_validate, reverse_schema) and the Rust competitor CLIs (jsonschema-cli,
# cargo-typify) are built by the driver and are not listed.
required_external_tools() {
    printf '%s\n' hyperfine node npm uv
}

# _install_hint <cmd>: how to install a required tool.
_install_hint() {
    case "$1" in
        hyperfine)      echo "cargo install hyperfine" ;;
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
# <cmd> is a program on PATH or a path; jsonschema-cli and cargo-typify name the driver's installs.
tool_version() {
    local raw
    case "$1" in
        # As a cargo subcommand binary, cargo-typify expects its subcommand name first.
        cargo-typify)   raw="$("$(_typify_bin)" typify --version 2>&1 || true)" ;;
        jsonschema-cli) raw="$("$(_jsonschema_cli_bin)" --version 2>&1 || true)" ;;
        *)              raw="$("$1" --version 2>&1 || true)" ;;
    esac
    printf '%s\n' "${raw}" | awk '
        { for (i = 1; i <= NF; i++) if ($i ~ /^v?[0-9]+\.[0-9]+/) { sub(/^v/, "", $i); print $i; found = 1; exit } }
        END { if (!found) print "unknown" }'
}

# --- Rust competitor CLIs ---------------------------------------------------------------

# profile_summary: the benchmark profile's own settings, comma-separated as profile.toml writes them
# (`inherits` left out), e.g. `codegen-units = 1, lto = "thin"`.
profile_summary() {
    awk '
        /^\[/ { in_profile = ($0 == "[profile.benchmark]"); next }
        in_profile && /=/ && $1 != "inherits" { sub(/[[:space:]]*#.*/, ""); out = out sep $0; sep = ", " }
        END { print out }' "$(_profile_config)"
}

# install_rust_cli_competitors <jsonschema version> <typify version>: install jsonschema-cli and
# cargo-typify at the releases the benchmark links (so both tiers measure one release), built with
# the benchmark profile like every other Rust tool, into _rust_tools_root. `--locked` builds each
# with the dependency versions its release was published with. cargo install skips a crate already
# installed at that version with the same toolchain; a changed profile is not something it tracks,
# so the profile's settings are stamped next to the installs and a change reinstalls both.
install_rust_cli_competitors() {
    local root stamp profile force=""
    root="$(_rust_tools_root)"
    stamp="${root}/profile.stamp"
    profile="$(profile_summary)"
    # No stamp: an install of unknown profile (or none) may sit in the root.
    if [[ ! -f "${stamp}" ]] || [[ "$(cat "${stamp}")" != "${profile}" ]]; then
        force="--force"
    fi
    cargo install --locked ${force} --root "${root}" \
        --config "$(_profile_config)" --profile benchmark "jsonschema-cli@$1" &&
        cargo install --locked ${force} --root "${root}" \
            --config "$(_profile_config)" --profile benchmark "cargo-typify@$2" &&
        printf '%s\n' "${profile}" > "${stamp}"
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
        jsonschemars | jsonschema | jsonschema-cli | boon | typify | valico | jsonschema-valid \
            | schemafy | jsonschema_code_generator | corvus-json-schema | schemars)
            echo "Rust (native)" ;;
        jsonschema-macros)
            echo "Rust (native); validators generated at compile time" ;;
        blaze)
            echo "C++ (native)" ;;
        ajv)
            echo "JavaScript (Node.js $(tool_version node))" ;;
        quicktype | json-schema-to-typescript | zod | ts-json-schema-generator)
            echo "TypeScript (Node.js $(tool_version node))" ;;
        python-jsonschema | fastjsonschema | datamodel-code-generator)
            echo "Python (CPython $(tool_version "$(_python_bin)"))" ;;
        pydantic)
            echo "Python (CPython $(tool_version "$(_python_bin)")) with a Rust core" ;;
        hyperfine)
            echo "Rust (native); times the CLI tier" ;;
        rustc)
            echo "compiles every Rust tool (one profile: $(profile_summary))" ;;
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
reverse_codegen_tools() {
    printf '%s\n' "jsonschemars schemars pydantic zod ts-json-schema-generator"
}

# --- command lines ---------------------------------------------------------------------

# loads_resources <tool> <operation>: true when <tool>'s CLI can take a fixture's resource
# documents for <operation> (codegen, validate_valid, validate_invalid, reverse_codegen). For
# reverse_codegen it is the model generator that takes them; the Rust models are compiled into
# reverse_schema, which reports a fixture it has no model for itself.
loads_resources() {
    case "$1:$2" in
        jsonschemars:*)         return 0 ;;
        schemars:reverse_codegen) return 0 ;;
        zod:reverse_codegen)    return 0 ;;
        ts-json-schema-generator:reverse_codegen) return 0 ;;
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
            _quote_argv "$(_typify_bin)" typify "${schema}" --output "${out_dir}/${base}.rs"
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
            _quote_argv "$(_jsonschema_cli_bin)" validate "${schema}" -i "${payload}"
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

# reverse_model_dir <tool> <fixture id>: the git-ignored directory <tool>'s generated models for
# <fixture id> go to. zod's must sit under competitors/node so its `import ... from "zod"`
# resolves to the pinned package; the others sit next to their runtimes for symmetry.
reverse_model_dir() {
    local fid="${2//\//__}"
    case "$1" in
        pydantic)                       printf '%s/reverse-models/%s\n' "$(_python_dir)" "${fid}" ;;
        zod | ts-json-schema-generator) printf '%s/reverse-models/%s/%s\n' "$(_node_dir)" "$1" "${fid}" ;;
        *)                              printf '\n' ;;
    esac
}

# reverse_model_cmd <tool> <schema> <model_dir> [resource...]: echo the command line generating
# <tool>'s models for <schema> into <model_dir> (which must already exist), or nothing for a tool
# whose models are compiled into reverse_schema. Never timed. Returns 2 for an unknown tool, 3 for
# resources the generator cannot take.
reverse_model_cmd() {
    local tool="$1" schema="$2" model_dir="$3" resource
    local -a resource_args
    shift 3
    resource_args=()
    for resource in ${1+"$@"}; do
        resource_args+=(-S "${resource}")
    done
    case "${tool}" in
        jsonschemars | schemars)
            ;;
        pydantic)
            _no_resources "${tool}" reverse_codegen "$#" || return
            _quote_argv "$(_python_dir)/.venv/bin/datamodel-codegen" --input "${schema}" \
                --input-file-type jsonschema --output-model-type pydantic_v2.BaseModel \
                --formatters builtin --class-name Root --disable-timestamp \
                --output "${model_dir}/models.py"
            ;;
        zod)
            _quote_argv "$(_node_dir)/node_modules/.bin/quicktype" --src-lang schema \
                --lang typescript-zod --top-level Root --out "${model_dir}/models.ts" "${schema}" \
                ${resource_args[@]+"${resource_args[@]}"}
            ;;
        ts-json-schema-generator)
            _quote_argv "$(_node_dir)/node_modules/.bin/quicktype" --src-lang schema \
                --lang typescript --just-types --explicit-unions --top-level Root \
                --out "${model_dir}/types.ts" \
                "${schema}" ${resource_args[@]+"${resource_args[@]}"}
            ;;
        *)
            echo "reverse_model_cmd: unknown reverse-codegen tool '${tool}'" >&2
            return 2
            ;;
    esac
}

# reverse_model_check <tool> <model_dir>: true when the generated models define the root model
# the timed command asks for; otherwise print why and return 1. A generator can succeed yet emit
# no root (quicktype names no type for a root that is only a map, such as `{"type": "object"}`, and
# emits no root zod schema for a root union), or emit TypeScript that does not type-check, which
# ts-json-schema-generator (it type-checks its input) would report as its own failure.
reverse_model_check() {
    local out
    case "$1" in
        jsonschemars | schemars)
            return 0 ;;
        pydantic)
            grep -Eq '^class Root\(' "$2/models.py" 2>/dev/null && return 0
            echo "datamodel-code-generator emitted no root model (Root)" ;;
        zod)
            grep -Eq '^export const RootSchema([^[:alnum:]_]|$)' "$2/models.ts" 2>/dev/null && return 0
            echo "quicktype emitted no root schema (RootSchema)" ;;
        ts-json-schema-generator)
            if ! grep -Eq '^export (interface|type|enum) Root([^[:alnum:]_]|$)' "$2/types.ts" 2>/dev/null; then
                echo "quicktype emitted no root type (Root)"
            elif ! out="$("$(_node_dir)/node_modules/.bin/tsc" --noEmit --pretty false "$2/types.ts" 2>&1)"; then
                echo "quicktype's TypeScript does not type-check: $(printf '%s\n' "${out}" \
                    | grep -m 1 'error TS' | sed 's/^.*: error /error /')"
            else
                return 0
            fi ;;
        *)
            echo "unknown reverse-codegen tool '$1'" ;;
    esac
    return 1
}

# reverse_codegen_cmd <tool> <fixture id> <model_dir>: echo the command line printing the JSON
# Schema of the fixture's root model (exit 0 = printed). Returns 2 for an unknown tool.
reverse_codegen_cmd() {
    local tool="$1" fid="$2" model_dir="$3"
    case "${tool}" in
        jsonschemars | schemars)
            _quote_argv "$(_reverse_schema_bin)" "${tool}" "${fid}"
            ;;
        pydantic)
            _quote_argv "$(_python_bin)" "$(_python_dir)/pydantic_schema.py" \
                "${model_dir}/models.py" Root
            ;;
        zod)
            _quote_argv node "$(_node_dir)/zod_schema.mjs" "${model_dir}/models.ts" RootSchema
            ;;
        ts-json-schema-generator)
            _quote_argv "$(_node_dir)/node_modules/.bin/ts-json-schema-generator" \
                --path "${model_dir}/types.ts" --type Root
            ;;
        *)
            echo "reverse_codegen_cmd: unknown reverse-codegen tool '${tool}'" >&2
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

# --- generated code size ---------------------------------------------------------------

# measure_code_size <jsonschemars> <list_fixtures> <code_size> <work dir> <output tsv>: generate
# every fixture with the given `jsonschemars`, with default settings and with constrained
# newtypes off, into <work dir>/<settings>/<fixture id, '/' as "__">/, and measure the generated
# code with <code_size> into <output tsv> (#113). A fixture whose codegen fails is reported and
# left out; one with unresolved references is skipped, as in the CLI tier.
measure_code_size() {
    local bin="$1" list_fixtures="$2" code_size="$3" work="$4" output="$5"
    local fid fdir resource_count unresolved resource settings out cmd
    local -a resources
    rm -rf "${work}"
    while IFS="$(printf '\t')" read -r fid fdir resource_count unresolved; do
        [[ -n "${fid}" && -z "${unresolved}" ]] || continue
        resources=()
        if [[ "${resource_count:-0}" -gt 0 ]]; then
            while IFS= read -r resource; do
                [[ -n "${resource}" ]] && resources+=("${resource}")
            done <<< "$(find "${fdir}/resources" -mindepth 1 -maxdepth 1 -name '*.json' | LC_ALL=C sort)"
        fi
        for settings in default no-newtypes; do
            out="${work}/${settings}/${fid//\//__}"
            mkdir -p "${out}"
            cmd="$(_quote_argv "${bin}" generate rust -o "${out}" "${fdir}/schema.json" \
                ${resources[@]+"${resources[@]}"})"
            if [[ "${settings}" == no-newtypes ]]; then
                cmd="${cmd} --cgs-constrained-newtypes false"
            fi
            if ! run_cmd_offline "${cmd}" > /dev/null 2>&1; then
                echo "    ${fid} (${settings}): codegen failed; its generated code is not measured" >&2
                rm -rf "${out}"
            fi
        done
    done <<< "$("${list_fixtures}")"
    "${code_size}" --generated "${work}" --output "${output}"
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
        schemars)       echo "schemars" ;;
        pydantic)       echo "pydantic" ;;
        zod)            echo "zod" ;;
        ts-json-schema-generator) echo "ts-json-schema-generator" ;;
        *)              echo "$1" ;;
    esac
}

# --- isolated/'s models -------------------------------------------------------------------

# build_models <manifest> <features> <exclude file> <log dir> [cargo args...]: build the isolated
# crate at <manifest> with the model features <features> (`reverse-codegen`, `generated-types`, or
# both, comma-separated; every bin), excluding each model that does not compile. Our codegen
# emits every fixture's Rust types (see isolated/models/gen); a model that does not compile is
# that cell's compat failure, not a broken build. `cargo check` finds them (no code generation, so
# each round is fast): after each failed check, model_build_failures reads cargo's JSON messages
# (<log dir>/model_build.jsonl) and appends one "copy \t fixture \t reason" row per failing model
# to <exclude file>, which the model shards' build scripts read through BENCH_MODEL_EXCLUDE to
# leave those models out (--keep-going, so every shard reports its errors in the same round). Rows
# already in the file stay (the quick benchmark pre-excludes models it does not measure). An error
# it cannot attribute to one model aborts the run. Once the check passes, one benchmark-profile
# build follows. BENCH_MODEL_EXCLUDE stays exported so a later build (the criterion bench) sees the
# same exclusions. CARGO_TARGET_DIR comes from the caller's environment; [cargo args...] go to
# every cargo command (the quick benchmark's path remapping). Returns 1 on failure: callers run it
# as `build_models ... || exit 1`, which suspends `set -e` inside it, so every step checks itself.
# clear_exclusions <exclude file>: leave <exclude file> empty, writing it only when it is missing
# or holds rows. The model shards' build scripts rerun whenever its modification time changes
# (rerun-if-changed), and a rerun rewrites every model and rebuilds every shard: emptying an
# already empty file made every run rebuild both sides' models from scratch (#112).
clear_exclusions() {
    if [[ ! -e "$1" || -s "$1" ]]; then
        : > "$1"
    fi
}

build_models() {
    local manifest="$1" features="$2" exclude="$3" log_dir="$4" attempt before after
    local triage="${REPO_ROOT:?REPO_ROOT not set}/target/benchmark/model_build_failures"
    shift 4
    export BENCH_MODEL_EXCLUDE="${exclude}"
    mkdir -p "${log_dir}"
    [[ -e "${exclude}" ]] || : > "${exclude}"
    for attempt in 1 2 3 4 5; do
        if cargo check --manifest-path "${manifest}" --config "$(_profile_config)" \
            --profile benchmark --features "${features}" --bins --keep-going \
            --message-format=json "$@" > "${log_dir}/model_build.jsonl"; then
            if [[ -s "${exclude}" ]]; then
                echo "    excluded models:"
                sed 's/^/      /' "${exclude}"
            fi
            cargo build --manifest-path "${manifest}" --config "$(_profile_config)" \
                --profile benchmark --features "${features}" --bins "$@" || return 1
            return 0
        fi
        echo "    model check ${attempt} failed; excluding the models that did not compile"
        before="$(wc -l < "${exclude}")"
        if ! "${triage}" < "${log_dir}/model_build.jsonl" \
            >> "${exclude}" 2> "${log_dir}/model_build_failures.err"; then
            echo "!! the model check failed for a reason no single model explains; aborting:" >&2
            cat "${log_dir}/model_build_failures.err" >&2
            return 1
        fi
        after="$(wc -l < "${exclude}")"
        if [[ "${after}" -le "${before}" ]]; then
            echo "!! the model check failed but no new model was excluded; aborting" \
                "(see ${log_dir}/model_build.jsonl)" >&2
            return 1
        fi
    done
    echo "!! the model check still fails after 5 attempts; aborting" \
        "(see ${log_dir}/model_build.jsonl)" >&2
    return 1
}
