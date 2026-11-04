# json-schema-rs-benchmark

Automated benchmark harness that measures `json-schema-rs` against competitor JSON Schema
libraries on the same fixtures, driven by a single `make benchmark` target, and marks a
per-category winner. This crate is a workspace member and is **not published**
(`publish = false`).

> Running the harness and committing its results is a **hard requirement** for every PR.
> See the repository [CLAUDE.md](../CLAUDE.md).

## Run it

```sh
make benchmark          # from the repo root
```

The external tools must be installed first, or the run aborts up front with install hints (a
partial run would silently omit a competitor and misrepresent the comparison):

```sh
cargo install hyperfine                # CLI-tier timing
cargo install cargo-typify --locked    # codegen competitor (typify)
cargo install jsonschema-cli --locked  # validation competitor (Stranger6667/jsonschema)
brew install node uv                   # runtimes for the non-Rust competitors
```

The non-Rust competitors need no manual install: the driver installs them from the committed lock
files, `npm ci` in `competitors/node/` and `uv sync --frozen` in `competitors/python/` (uv also
fetches the Python pinned in `.python-version`), into git-ignored directories.

Competitors must also be their newest stable release: the run aborts with the upgrade command
when a linked crate (`jsonschema`, `typify`, `boon`, `valico`, `jsonschema-valid`, `schemafy_lib`,
`jsonschema_code_generator` in `Cargo.lock`; `corvus-json-schema` and `jsonschema` in
`isolated/Cargo.lock`) is behind crates.io, when
a competitor CLI is not the same release as its crate, or when an npm or PyPI competitor's pin is
behind the registry. For a crate, bump the version in its `Cargo.toml`, run `cargo update -p <crate>` (with
`--manifest-path isolated/Cargo.toml` for the isolated crates),
and reinstall the CLI at that version; for npm, `npm install --save-exact <package>@<version>` in
`competitors/node/`; for PyPI, `uv add <package>==<version>` in `competitors/python/`.

Our own `jsonschemars` CLI, the `boon` wrapper and every in-process competitor (linked crates)
are built by the driver.

Outputs, committed for PR review:

- `results.json` — raw measurements (median, stddev, min, max, samples), compatibility matrix,
  tool versions.
- `results.md` — methodology, category winners, one table per tier/operation/metric, and the
  compatibility matrix.

## What it measures

Two tiers, on shared fixtures (committed small / medium / large / massive, plus large synthetic
fixtures generated deterministically into `target/benchmark-fixtures/` on every run):

| Tier | How | Tools | Time | Memory |
|---|---|---|---|---|
| In-process | every tool linked as a library, called on in-memory inputs | validators: json-schema-rs, [jsonschema] (runtime, and its compile-time macro as `jsonschema-macros`), [boon], [valico], [jsonschema-valid], [corvus-json-schema]; codegens: json-schema-rs, [typify], [schemafy], [jsonschema_code_generator] | [criterion] median ± stddev | peak heap per operation (counting allocator) |
| CLI | every tool's binary end to end, startup included | Rust: `jsonschemars`, `jsonschema-cli`, `boon_validate`, `cargo-typify`; non-Rust: [Blaze], [ajv], [python-jsonschema], [fastjsonschema], [quicktype], [datamodel-code-generator], [json-schema-to-typescript] | [hyperfine] median ± stddev | peak RSS |

Operations: `codegen` (schema → source; Rust for every tool but datamodel-code-generator, which
emits pydantic, and json-schema-to-typescript, which emits TypeScript), `schema_compile`
(in-process only; not for `jsonschema-macros`, whose schemas compile when the benchmark is built),
`validate_valid`, `validate_invalid`. Lower is better everywhere.

`corvus-json-schema` and `jsonschema-macros` are built in [`isolated/`](isolated/Cargo.toml), a crate
outside the workspace, one build each: corvus turns on serde_json's `preserve_order`, which would
otherwise reach our library and every other tool, and the compile-time validators are generated
from the fixture set, which only exists once the driver has generated it. Both builds append to the
same raw captures as the main crate, so the report shows every tool side by side.

Non-Rust tools are CLI-tier only: in-process numbers across languages are not comparable. Node
and Python start slowly, so every CLI table opens with a *startup* row (each tool on a trivial
draft-07 `{"type": "object"}` schema) that shows each tool's fixed per-process cost and never
counts toward a winner. The report's Tools table lists every tool's version and language/runtime.
ajv, python-jsonschema and fastjsonschema have no usable CLI, so thin wrappers in `competitors/`
call them the way `boon_validate` calls boon (see design.md for why each exists and the options
they set).

## Rules baked into the harness

- A cell is measured only when the tool is **correct** on it (codegen succeeds, schema compiles,
  valid accepted, invalid rejected); anything else is a compatibility failure with a reason.
- The run is **hermetic**. A fixture may vendor the documents its schema references under
  `resources/` (each with an absolute root `$id`); every tool gets them registered under that
  `$id` the way it accepts extra documents (in-process: our `$ref` registry / an in-memory
  retriever for codegen, a `jsonschema` registry, boon's `add_resource`; CLI: extra
  `jsonschemars generate` inputs, `jsonschemars validate -r`, `boon_validate -r`). A tool that
  cannot take extra documents (typify, schemafy, jsonschema_code_generator, jsonschema-valid,
  `jsonschema-cli`) fails that cell with "cannot load the
  fixture's resource documents". A fixture with a `$ref` to any document it does not vendor
  (checked across the schema and its resources) is not measured at all. In-process, our codegen
  reaches the resources through a retriever, so only the schema gets root types (as in every
  validator's registry); our CLI has no such option, so it takes them as extra inputs and also
  writes each resource's own types.
- A tool **wins** a cell only when at least two tools were measured and its lead beats both 3%
  and the sum of the two standard deviations; ties and single-tool cells are never bolded. `†`
  flags a measurement whose stddev exceeds 10% of its median.
- If **our** library fails a valid schema, that is filed as a GitHub issue (we aim to support
  100% of valid JSON Schema).

See [design.md](../design.md#benchmark-harness) for the full methodology.

## Env knobs

| Variable | Default | Effect |
|---|---|---|
| `BENCH_CRITERION_ARGS` | `--warm-up-time 0.5 --measurement-time 2 --sample-size 20` | criterion arguments |
| `BENCH_SKIP_CRITERION=1` | unset | skip in-process timing |
| `BENCH_CLI_WARMUP` | `3` | hyperfine warmup runs per CLI cell |
| `BENCH_CLI_RUNS` | `15` | hyperfine timed runs per CLI cell |

## Layout

```
json_schema_rs_benchmark/
  src/lib.rs               # results model, statistics, winner rule, report rendering
  src/harness.rs           # the Adapter trait, correctness check, heap profiler, criterion runner
  src/adapters.rs          # every tool this crate links, behind the Adapter trait (in-process tier)
  src/fixtures.rs          # fixture discovery shared by both tiers
  src/bin/aggregate.rs     # raw captures -> results.{json,md}
  src/bin/heap_profile.rs  # per-operation peak heap + in-process compatibility
  src/bin/gen_fixtures.rs  # deterministic large synthetic fixtures
  src/bin/list_fixtures.rs # fixture list for the CLI tier
  src/bin/boon_validate.rs # thin boon CLI wrapper (boon-cli cannot register documents by $id)
  benches/cross_tool.rs    # criterion timing of every tool
  isolated/                # corvus-json-schema and jsonschema-macros, built apart (own workspace);
                           #   build.rs generates one compile-time validator per fixture
  scripts/run_benchmark.sh # driver invoked by `make benchmark`
  scripts/tools.sh         # CLI-tier tool definitions
  competitors/node/        # pinned npm competitors (package.json + lock) + ajv_validate.mjs
  competitors/python/      # pinned PyPI competitors (pyproject.toml + uv.lock) + validation wrappers
  fixtures/<size>/<name>/  # schema.json, instances/{valid,invalid}/*.json, optional resources/*.json
  fixtures/manifest.json   # provenance and license of every committed schema, instance and resource
  results.json, results.md # committed aggregated results
```

[typify]: https://github.com/oxidecomputer/typify
[valico]: https://github.com/s-panferov/valico
[jsonschema-valid]: https://github.com/mdboom/jsonschema-valid
[corvus-json-schema]: https://github.com/corvus-dotnet/Corvus.JsonSchema
[schemafy]: https://github.com/Marwes/schemafy
[jsonschema_code_generator]: https://github.com/tim-hellhake/jsonschema_code_generator
[jsonschema]: https://github.com/Stranger6667/jsonschema
[boon]: https://github.com/santhosh-tekuri/boon
[criterion]: https://github.com/bheisler/criterion.rs
[hyperfine]: https://github.com/sharkdp/hyperfine
[Blaze]: https://github.com/sourcemeta/blaze
[ajv]: https://github.com/ajv-validator/ajv
[python-jsonschema]: https://github.com/python-jsonschema/jsonschema
[fastjsonschema]: https://github.com/horejsek/python-fastjsonschema
[quicktype]: https://github.com/glideapps/quicktype
[datamodel-code-generator]: https://github.com/koxudaxi/datamodel-code-generator
[json-schema-to-typescript]: https://github.com/bcherny/json-schema-to-typescript
