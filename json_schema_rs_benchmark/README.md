# json-schema-rs-benchmark

Automated benchmark harness that measures `json-schema-rs` against competitor JSON Schema
libraries on the same fixtures and marks a per-category winner. This crate is a workspace
member and is **not published** (`publish = false`).

> Running `make benchmark` and putting its report in the PR body is a **hard requirement** for
> every PR that changes the library, CLI or macros. See the repository [CLAUDE.md](../CLAUDE.md).

## Run it

```sh
make benchmark          # quick, every library PR: our library + CLI, main vs the working tree (~5-10 min)
make benchmark_full     # full: every tool, refreshes results.{json,md} (~60 min)
```

### Quick mode (`make benchmark`)

Measures only our library and CLI (`jsonschemars`), on two builds: the base commit (the merge
base of `main` and `HEAD`; set `BENCH_BASE` for another ref) and the working tree, uncommitted
changes included. Both sides are built with the benchmark profile, the base from a `git archive`
of its commit in `target/benchmark-quick/base-src` with the working tree's `Cargo.lock`, and
measured in the same session, interleaved cell by cell: in-process peak heap once per side (it is
exact), then criterion one cell at a time (`BENCH_CELL`), each cell's `BENCH_QUICK_ROUNDS` rounds
(default 2) back to back in ABBA order (base, head, head, base), then the CLI tier (hyperfine:
codegen, validate_valid, validate_invalid, and the startup row) in the same number of rounds, each
running every cell's two sides back to back. Both sides' timed executables run from copies at
paths of the same length (`target/benchmark-quick/run/{base,head}/`).

The `compare` binary then prints which cells moved beyond noise as the PR body's *Performance*
section, and writes it to `target/benchmark-quick/report.md`. Each round (one process) of a cell
is summarized by its **lowest sample**; each side by the **median of its rounds' lowest samples**,
with their standard deviation as the spread. A cell moved when the two sides differ by more than
3% of `main`'s value and by more than the sum of both spreads. Every cell that moved is then
measured again in `BENCH_QUICK_CONFIRM_ROUNDS` further rounds per side (default 4), in the same
order, and the report judges it on all its rounds: a real change moves every process, while a
false move comes from two rounds whose spread happened to be small. A cell that passes its
correctness probe on one side only is listed apart. Nothing committed changes.

Why this design: the machine is shared (other builds, benchmarks, sessions), and three effects
moved cells with no code change (#100), each measured with A/A runs (both sides the same commit):

| Effect | What it did | Countered by |
|---|---|---|
| Drift: speed changes over minutes, load arrives in bursts of seconds | results from another day differ by up to 10% in-process; whole-tier rounds a minute apart disagreed on 44 of 48 in-process cells | interleaving per cell (5 of 48), the lowest sample per round |
| Paths: the executable's path sits on the process's stack; a path compiled into the binary shifts its layout | the same binary from a longer path ran a sub-microsecond cell 5.5% slower in all 8 processes; builds of one commit from two checkouts differed by up to 8% on one cell | equal-length executable paths; no absolute path compiled into the bench (the default fixture directory is read at run time) |
| Per-process layout: each process gets its own memory layout | some sub-microsecond cells run about 4% faster in some processes | the median across rounds, with their spread as the noise; confirmation rounds for every cell that moved |

The original design (whole-tier rounds, pooled medians, unequal paths) reported 3 of 174 cells
moved in an A/A run (up to +12%). Two A/A runs of this one reported none: 2 to 4 cells per run
moved after the first two rounds, and their confirmation rounds cleared every one. On A/A
captures with injected changes, the per-round rule caught 88% of cells at +4%, 93% at +5% and
all at +10% (pooled medians: 83%, 90%, 99.5%). A run takes about 6-7.5 minutes. A cell that moved but runs only code the PR did not touch is
still noise; the PR body counts only cells on changed code paths (see CLAUDE.md).

The generated-types cells (see *What it measures*) come from each side's own build of
`isolated/`'s `generated-types` feature: that side's codegen generates the types, which link that
side's library. Each side builds into `target/benchmark-quick/isolated-{base,head}` with every
source and target path remapped (`--remap-path-prefix`), because `isolated/` is its own workspace
and cargo compiles the library under its absolute path, which is longer on the base side.
Reverse codegen is not measured in quick mode: its models take minutes to build per side (#102).
A PR that changes the derive or reverse codegen, the harness, a competitor or a fixture runs
`make benchmark_full` too.

The first quick run builds the base side's dependencies from scratch into
`target/benchmark-quick/base-target`; later runs only rebuild what changed.

### Full mode (`make benchmark_full`)

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
| In-process | every tool linked as a library, called on in-memory inputs | validators: json-schema-rs, [jsonschema] (runtime, and its compile-time macro as `jsonschema-macros`), [boon], [valico], [jsonschema-valid], [corvus-json-schema]; codegens: json-schema-rs, [typify], [schemafy], [jsonschema_code_generator]; reverse codegen: json-schema-rs, [schemars]; generated types: json-schema-rs | [criterion] median ± stddev | peak heap per operation (counting allocator) |
| CLI | every tool's binary end to end, startup included | Rust: `jsonschemars`, `jsonschema-cli`, `boon_validate`, `cargo-typify`, `reverse_schema` (json-schema-rs and schemars); non-Rust: [Blaze], [ajv], [python-jsonschema], [fastjsonschema], [quicktype], [datamodel-code-generator], [json-schema-to-typescript], [pydantic], [zod], [ts-json-schema-generator] | [hyperfine] median ± stddev | peak RSS |

Operations: `codegen` (schema → source; Rust for every tool but datamodel-code-generator, which
emits pydantic, and json-schema-to-typescript, which emits TypeScript), `schema_compile`
(in-process only; not for `jsonschema-macros`, whose schemas compile when the benchmark is built),
`validate_valid`, `validate_invalid`, `reverse_codegen` (language-native models → JSON Schema:
`#[derive(ToJsonSchema)]`'s `T::json_schema()`, schemars' `schema_for!`, pydantic's
`model_json_schema()`, zod v4's `z.toJSONSchema`, ts-json-schema-generator's CLI), and the
generated-types operations (ours only, in-process): the fixture's instance deserialized into the
types our codegen generates for its schema, `deserialize_valid` (plain `serde_json::from_str` of
the root type), `try_from_valid` and `try_from_invalid` (the root type's
`TryFrom<serde_json::Value>`, which validates the whole instance; the instance's clone is made
outside the timing), each also `*_no_newtypes` on types generated with
`constrained_newtypes(false)`. Lower is better everywhere.

`reverse_codegen` and the generated-types operations need a model per fixture, so the models are
generated from the fixture's schema at bench time, deterministically, and are never timed or
committed: our codegen emits the Rust types, one copy per tool carrying that tool's derive, plus
a copy without constrained newtypes (compiled into `isolated/`; a model that does not compile is
excluded from the build and recorded as that tool's compat failure), datamodel-code-generator emits
the pydantic models (`--class-name Root`), and quicktype emits the zod schemas
(`--lang typescript-zod`) and the TypeScript types for ts-json-schema-generator
(`--lang typescript --just-types --explicit-unions`, so a root union gets a name), registering the
fixture's resources with `-S` (datamodel-code-generator cannot take them). A generator that fails,
or emits models without the root type (quicktype names no root that is only a map, such as the
startup schema, and emits no root zod schema for a root union) or TypeScript that does not
type-check, is a compat failure for that tool and fixture. Before a cell is timed its emitted
schema must be accepted by our validator and accept every valid instance of the fixture, so a
fast-but-wrong schema is never timed. A generated-types cell is measured only when the types accept
the valid instance and (`try_from_invalid`) reject the invalid one. The fixtures in
`MODELS_NOT_BUILT` (`src/models.rs`) have no model, so no reverse-codegen or generated-types
cell: their types take too long to build (`generated/wide-object`, #102;
`medium/package-json`, #111).
The CLI tier times one process per run: `reverse_schema`
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

Defaults are given as full mode / quick mode.

| Variable | Default | Effect |
|---|---|---|
| `BENCH_CRITERION_ARGS` | `--warm-up-time 0.5 --measurement-time 2 --sample-size 20` / `--warm-up-time 0.3 --measurement-time 1 --sample-size 20 --noplot` | criterion arguments (per cell and round in quick mode) |
| `BENCH_SKIP_CRITERION=1` | unset (full mode only) | skip in-process timing |
| `BENCH_CLI_WARMUP` | `3` / `2` | hyperfine warmup runs per CLI cell (and round) |
| `BENCH_CLI_RUNS` | `15` / `10` | hyperfine timed runs per CLI cell (and round) |
| `BENCH_BASE` | — / `main` | quick mode: the ref whose merge base with `HEAD` is the base side |
| `BENCH_QUICK_ROUNDS` | — / `2` | quick mode: interleaved rounds per side |
| `BENCH_QUICK_CONFIRM_ROUNDS` | — / `4` | quick mode: further rounds per side for each cell that moved (`0` skips) |
| `BENCH_TOOLS` | unset | comma-separated tool ids the in-process harness measures (quick mode sets `jsonschemars`) |
| `BENCH_CELL` | unset | one in-process cell the criterion bench checks and times, `<operation>/<fixture id>` (quick mode sets it per cell) |

## Layout

```
json_schema_rs_benchmark/
  src/lib.rs               # results model, statistics, winner rule, report rendering
  src/harness.rs           # the Adapter trait, correctness check, heap profiler, criterion runner
  src/adapters.rs          # every tool this crate links, behind the Adapter trait (in-process tier)
  src/fixtures.rs          # fixture discovery shared by both tiers
  src/bin/aggregate.rs     # raw captures -> results.{json,md}
  src/bin/heap_profile.rs  # per-operation peak heap + in-process compatibility
  src/bin/compare.rs       # quick mode: main vs the working tree, cells moved beyond noise
  src/bin/gen_fixtures.rs  # deterministic large synthetic fixtures
  src/bin/list_fixtures.rs # fixture list for the CLI tier
  src/bin/boon_validate.rs # thin boon CLI wrapper (boon-cli cannot register documents by $id)
  src/bin/model_build_failures.rs # cargo JSON messages -> models to exclude
  benches/cross_tool.rs    # criterion timing of every tool
  isolated/                # corvus-json-schema, jsonschema-macros and the models (reverse_codegen,
                           #   generated types), built apart (own workspace, one build per feature;
                           #   the two model features build together); build.rs generates one
                           #   compile-time validator per fixture; models/ holds the model
                           #   generator (gen/) and the 8 shard crates the models are spread over,
                           #   so they compile in parallel; src/bin/reverse_schema.rs is the
                           #   reverse_codegen CLI-tier wrapper
  scripts/run_benchmark.sh # full-mode driver (`make benchmark_full`)
  scripts/quick_benchmark.sh # quick-mode driver (`make benchmark`)
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
