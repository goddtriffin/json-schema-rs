# json-schema-rs-benchmark

Automated benchmark harness that measures `json-schema-rs` against competitor JSON Schema
libraries on the same fixtures, driven by a single `make benchmark` target, and marks a
per-category winner. This crate is a workspace member and is **not published**
(`publish = false`).

> Running the harness and committing its results is a **hard requirement** for every PR.
> See the repository [CLAUDE.md](../CLAUDE.md).

## Run it

```sh
make benchmark          # from the repo root (research_benchmark is a backwards-compat alias)
```

The external tools must be installed first, or the run aborts up front with install hints (a
partial run would silently omit a competitor and misrepresent the comparison):

```sh
cargo install hyperfine                # CLI-tier timing
cargo install cargo-typify --locked    # codegen competitor (typify)
cargo install jsonschema-cli --locked  # validation competitor (Stranger6667/jsonschema)
```

Competitors must also be their newest stable release: the run aborts with the upgrade command
when a linked crate (`jsonschema`, `typify`, `boon` in `Cargo.lock`) is behind crates.io, or when
a competitor CLI is not the same release as its crate. Bump the version in `Cargo.toml`, run
`cargo update -p <crate>`, and reinstall the CLI at that version.

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
| In-process | every tool linked as a library, called on in-memory inputs | json-schema-rs, [jsonschema], [boon], [typify] | [criterion] median ± stddev | peak heap per operation (counting allocator) |
| CLI | every tool's binary end to end, startup included | `jsonschemars`, `jsonschema-cli`, `boon_validate`, `cargo-typify` | [hyperfine] median ± stddev | peak RSS |

Operations: `codegen` (schema → Rust source), `schema_compile` (in-process only),
`validate_valid`, `validate_invalid`. Lower is better everywhere.

## Rules baked into the harness

- A cell is measured only when the tool is **correct** on it (codegen succeeds, schema compiles,
  valid accepted, invalid rejected); anything else is a compatibility failure with a reason.
- The run is **hermetic**. A fixture may vendor the documents its schema references under
  `resources/` (each with an absolute root `$id`); every tool gets them registered under that
  `$id` the way it accepts extra documents (in-process: our `$ref` registry / an in-memory
  retriever for codegen, a `jsonschema` registry, boon's `add_resource`; CLI: extra
  `jsonschemars generate` inputs, `boon_validate -r`). A tool that cannot take extra documents
  (typify, `jsonschema-cli`, `jsonschemars validate`) fails that cell with "cannot load the
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
  src/adapters.rs          # every tool behind one interface (in-process tier)
  src/fixtures.rs          # fixture discovery shared by both tiers
  src/bin/aggregate.rs     # raw captures -> results.{json,md}
  src/bin/heap_profile.rs  # per-operation peak heap + in-process compatibility
  src/bin/gen_fixtures.rs  # deterministic large synthetic fixtures
  src/bin/list_fixtures.rs # fixture list for the CLI tier
  src/bin/boon_validate.rs # thin boon CLI wrapper (boon has no CLI)
  benches/cross_tool.rs    # criterion timing of every tool
  scripts/run_benchmark.sh # driver invoked by `make benchmark`
  scripts/tools.sh         # CLI-tier tool definitions
  fixtures/<size>/<name>/  # schema.json, instances/{valid,invalid}/*.json, optional resources/*.json
  fixtures/manifest.json   # provenance and license of every committed schema, instance and resource
  results.json, results.md # committed aggregated results
```

[typify]: https://github.com/oxidecomputer/typify
[jsonschema]: https://github.com/Stranger6667/jsonschema
[boon]: https://github.com/santhosh-tekuri/boon
[criterion]: https://github.com/bheisler/criterion.rs
[hyperfine]: https://github.com/sharkdp/hyperfine
