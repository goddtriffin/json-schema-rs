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
brew install node uv                   # runtimes for the non-Rust competitors
```

The competitors need no manual install. The driver installs the Rust competitor CLIs
(`jsonschema-cli`, `cargo-typify`) at the release the benchmark links, into
`target/benchmark-tools/` (see *Build profile*), and the non-Rust competitors from the committed
lock files, `npm ci` in `competitors/node/` and `uv sync --frozen` in `competitors/python/` (uv also
fetches the Python pinned in `.python-version`), into git-ignored directories.

Competitors must also be their newest stable release: the run aborts with the upgrade command
when a linked crate (`jsonschema`, `typify`, `boon`, `valico`, `jsonschema-valid`, `schemafy_lib`,
`jsonschema_code_generator` in `Cargo.lock`; `corvus-json-schema`, `jsonschema` and `schemars` in
`isolated/Cargo.lock`) is behind crates.io, or when an npm (`@sourcemeta/jsonschema`, `ajv`,
`ajv-draft-04`, `quicktype`, `json-schema-to-typescript`, `zod`, `ts-json-schema-generator`) or PyPI
(`jsonschema`, `fastjsonschema`, `datamodel-code-generator`, `pydantic`) competitor's pin is behind
the registry. For a crate, bump the version in its `Cargo.toml` and run `cargo update -p <crate>` (with
`--manifest-path isolated/Cargo.toml` for the isolated crates); the driver installs the matching CLI
on the next run. For npm, `npm install --save-exact <package>@<version>` in
`competitors/node/`; for PyPI, `uv add <package>==<version>` in `competitors/python/`.

Our own `jsonschemars` CLI, the `boon` wrapper, the `reverse_schema` wrapper and every in-process
competitor (linked crates) are built by the driver.

### Build profile

Every Rust tool, ours and every competitor's alike, is built with one profile, `benchmark` in
[`profile.toml`](profile.toml): `codegen-units = 1` and `lto = "thin"` on top of `release`. The
driver passes that file to every cargo command it runs (`--config profile.toml --profile
benchmark`): the workspace builds and benches, both isolated builds, and the `cargo install` of
each competitor CLI. Binaries land in `target/benchmark/`. The profile is recorded in the results
(the `rustc` row of the Tools table).

Why: with Cargo's default 16 codegen units, which unit a hot function lands in, and therefore what
gets inlined into it, shifts when unrelated code is added to a crate. Adding a module the benchmark
never calls moved in-process validation by up to 12%, a "regression" no code path caused. With one
codegen unit and thin LTO the same comparison moves no cell beyond noise; without LTO or with fat
LTO a few cells still moved 3-5%. Thin LTO was also the fastest of the three for every tool.

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
| In-process | every tool linked as a library, called on in-memory inputs | validators: json-schema-rs, [jsonschema] (runtime, and its compile-time macro as `jsonschema-macros`), [boon], [valico], [jsonschema-valid], [corvus-json-schema]; codegens: json-schema-rs, [typify], [schemafy], [jsonschema_code_generator]; reverse codegen: json-schema-rs, [schemars] | [criterion] median ± stddev | peak heap per operation (counting allocator) |
| CLI | every tool's binary end to end, startup included | Rust: `jsonschemars`, `jsonschema-cli`, `boon_validate`, `cargo-typify`, `reverse_schema` (json-schema-rs and schemars); non-Rust: [Blaze], [ajv], [python-jsonschema], [fastjsonschema], [quicktype], [datamodel-code-generator], [json-schema-to-typescript], [pydantic], [zod], [ts-json-schema-generator] | [hyperfine] median ± stddev | peak RSS |

Operations: `codegen` (schema → source; Rust for every tool but datamodel-code-generator, which
emits pydantic, and json-schema-to-typescript, which emits TypeScript), `schema_compile`
(in-process only; not for `jsonschema-macros`, whose schemas compile when the benchmark is built),
`validate_valid`, `validate_invalid`, `reverse_codegen` (language-native models → JSON Schema:
`#[derive(ToJsonSchema)]`'s `T::json_schema()`, schemars' `schema_for!`, pydantic's
`model_json_schema()`, zod v4's `z.toJSONSchema`, ts-json-schema-generator's CLI). Lower is better
everywhere.

`reverse_codegen` needs a model per fixture, so the models are generated from the fixture's schema
at bench time, deterministically, and are never timed or committed: our codegen emits the Rust
types, one copy per tool carrying that tool's derive (compiled into `isolated/`; a model that does
not compile is excluded from the build and recorded as that tool's compat failure), datamodel-code-generator emits
the pydantic models (`--class-name Root`), and quicktype emits the zod schemas
(`--lang typescript-zod`) and the TypeScript types for ts-json-schema-generator
(`--lang typescript --just-types --explicit-unions`, so a root union gets a name), registering the
fixture's resources with `-S` (datamodel-code-generator cannot take them). A generator that fails,
or emits models without the root type (quicktype names no root that is only a map, such as the
startup schema, and emits no root zod schema for a root union) or TypeScript that does not
type-check, is a compat failure for that tool and fixture. Before a cell is timed its emitted
schema must be accepted by our validator and accept every valid instance of the fixture, so a
fast-but-wrong schema is never timed. The CLI tier times one process per run: `reverse_schema`
(our thin bin with every fixture's Rust models compiled in; `reverse_schema <jsonschemars|schemars>
<fixture>`), and the thin wrappers `competitors/python/pydantic_schema.py` and
`competitors/node/zod_schema.mjs` (pydantic and zod have no command line), each with the library's
default options.

`corvus-json-schema` and `jsonschema-macros` are built in [`isolated/`](isolated/Cargo.toml), a crate
outside the workspace, one build each: corvus turns on serde_json's `preserve_order`, which would
otherwise reach our library and every other tool, and the compile-time validators are generated
from the fixture set, which only exists once the driver has generated it. Both builds append to the
same raw captures as the main crate, so the report shows every tool side by side.

Non-Rust tools are CLI-tier only: in-process numbers across languages are not comparable. Node
and Python start slowly, so every CLI table opens with a *startup* row (each tool on a trivial
draft-07 object schema with one optional string property) that shows each tool's fixed per-process cost and never
counts toward a winner. The report's Tools table lists every tool's version and language/runtime.
ajv, python-jsonschema, fastjsonschema, pydantic and zod have no usable CLI, so thin wrappers in
`competitors/` call them the way `boon_validate` calls boon (see design.md for why each exists and
the options they set).

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

## Considered, not benchmarked

- [facet-jsonschema] 0.46 (reverse codegen through `#[derive(Facet)]` reflection) implements too
  little of the spec for a fair comparison: it panics with `todo!()` on every enum and map and
  `unimplemented!()` on every `Option`, and recurses without a limit.

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
  src/bin/model_build_failures.rs # cargo JSON messages -> reverse_codegen models to exclude
  benches/cross_tool.rs    # criterion timing of every tool
  isolated/                # corvus-json-schema, jsonschema-macros and the reverse_codegen models,
                           #   built apart (own workspace, one build per feature); build.rs generates
                           #   one compile-time validator and one Rust model per fixture;
                           #   src/bin/reverse_schema.rs is the reverse_codegen CLI-tier wrapper
  scripts/run_benchmark.sh # driver invoked by `make benchmark`
  scripts/tools.sh         # CLI-tier tool definitions
  competitors/node/        # pinned npm competitors (package.json + lock) + ajv_validate.mjs,
                           #   zod_schema.mjs; reverse-models/ (git-ignored) holds generated models
  competitors/python/      # pinned PyPI competitors (pyproject.toml + uv.lock) + validation wrappers,
                           #   pydantic_schema.py; reverse-models/ (git-ignored) holds generated models
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
[schemars]: https://github.com/GREsau/schemars
[facet-jsonschema]: https://github.com/facet-rs/facet
[pydantic]: https://github.com/pydantic/pydantic
[zod]: https://github.com/colinhacks/zod
[ts-json-schema-generator]: https://github.com/vega/ts-json-schema-generator
