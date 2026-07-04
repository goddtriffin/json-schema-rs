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

Outputs, committed for PR review:

- `results.json` — machine-readable aggregated results.
- `results.md` — human-readable table with the winner marked per category, plus the codegen
  compatibility matrix.

## What it measures

Two axes, on shared language-agnostic fixtures at small / medium / large / massive sizes:

- **Codegen** (schema → code): our lib vs [typify] (`cargo typify`) and [schemafy] (CLI).
- **Validation** (schema + instance → result): our lib vs [jsonschema-cli] and [boon].

Per-metric categories (lower is better for all):

| Category | Source | Scope |
|---|---|---|
| codegen wall-time | Hyperfine (CLI) | all codegen tools |
| schema-compile time | criterion (in-process) | our lib only |
| validate (valid) | Hyperfine (CLI) + criterion | all validation tools / our lib |
| validate (invalid) | Hyperfine (CLI) | all validation tools (error-path) |
| peak memory | `/usr/bin/time` peak RSS | all tools |

Wall-time comes from [Hyperfine]; our own in-process hot paths use [criterion] (with
optional [dhat] heap profiling under the `dhat-heap` feature) so we can split
schema-compile from per-instance validate and gate our own regressions. Peak memory is
captured uniformly across every tool with `/usr/bin/time` (macOS `-l` / Linux `-v`).

## Rules baked into the harness

- Perf numbers are only reported on fixtures that **every wired tool handles** (apples to
  apples). Each size tier has at least one such fixture.
- A tool that cannot generate code for a fixture is recorded in the **codegen compat
  matrix** as a failure (not timed) — never a hard error.
- If **our** library fails a valid schema, that is surfaced loudly and filed as a GitHub
  issue (we aim to support 100% of valid JSON Schema).
- Invalid-instance validation timing is measured and reported as a separate, labeled
  category (validators behave differently on the error path).

## Layout

```
json_schema_rs_benchmark/
  src/lib.rs              # shared results model + winner logic
  src/bin/aggregate.rs    # raw captures -> results.{json,md}
  src/bin/boon_validate.rs# thin boon CLI wrapper (so Hyperfine can time boon)
  benches/hot_paths.rs    # criterion micro-benchmarks of our hot paths
  scripts/run_benchmark.sh# driver invoked by `make benchmark`
  scripts/tools.sh        # per-competitor CLI invocation definitions
  fixtures/<size>/...     # schemas + instances, plus a manifest of tool support
  results.json, results.md# committed aggregated results
```

[typify]: https://github.com/oxidecomputer/typify
[schemafy]: https://github.com/Marwes/schemafy
[jsonschema-cli]: https://github.com/Stranger6667/jsonschema
[boon]: https://github.com/santhosh-tekuri/boon
[Hyperfine]: https://github.com/sharkdp/hyperfine
[criterion]: https://github.com/bheisler/criterion.rs
[dhat]: https://github.com/nnethercote/dhat-rs
