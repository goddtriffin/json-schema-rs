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

The competitor CLIs must be installed first, or the run aborts up front with install hints
(a partial run would silently omit a competitor and misrepresent the comparison):

```sh
cargo install cargo-typify    # codegen competitor (typify)
cargo install jsonschema-cli  # validation competitor (Stranger6667/jsonschema)
```

Our own `jsonschemars` CLI and the `boon` wrapper are built by the driver, so they need no
separate install.

Outputs, committed for PR review:

- `results.json` — machine-readable aggregated results.
- `results.md` — human-readable table with the winner marked per category, plus the codegen
  compatibility matrix.

## What it measures

Two axes, on shared language-agnostic fixtures at small / medium / large / massive sizes:

- **Codegen** (schema → code): our lib vs [typify] (`cargo typify`). ([schemafy] is a
  proc-macro with no CLI, so it cannot be wired as a codegen competitor.)
- **Validation** (schema + instance → result): our lib vs [jsonschema-cli] and [boon].

Per-metric categories (lower is better for all):

| Category | Source | Scope |
|---|---|---|
| codegen wall-time | `/usr/bin/time` (CLI) | all codegen tools |
| schema-compile time | criterion (in-process) | our lib only |
| validate (valid) | `/usr/bin/time` (CLI) + criterion | all validation tools / our lib |
| validate (invalid) | `/usr/bin/time` (CLI) | all validation tools (error-path) |
| peak memory | `/usr/bin/time` peak RSS | all tools |

Wall-time and peak memory both come from `/usr/bin/time` (macOS `-l` / Linux `-v`), which
reports `real` time and peak RSS in one shot for every tool uniformly. Our own in-process
hot paths additionally use [criterion] (with optional [dhat] heap profiling under the
`dhat-heap` feature) so we can split schema-compile from per-instance validate and gate our
own regressions.

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
  src/bin/boon_validate.rs# thin boon CLI wrapper (so the harness can time boon end-to-end)
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
[criterion]: https://github.com/bheisler/criterion.rs
[dhat]: https://github.com/nnethercote/dhat-rs
