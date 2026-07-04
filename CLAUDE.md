# CLAUDE.md — json-schema-rs

Operating rules for working in this repository. These are **hard requirements**, not
suggestions. They exist so every change stays aligned with our standards and our vision.
For consumer-facing information see [README.md](README.md); for design and architecture
see [design.md](design.md).

## Hard requirements for every code change

1. **Invoke the `contribution-guide` skill before writing or changing any code.** Always.
   It defines the implementation workflow, testing standards, and repository conventions.
   Do not begin an implementation without it.

2. **Refresh your understanding from [design.md](design.md) before implementing.** Read the
   section(s) relevant to what you are about to change (architecture, values, design
   principles, and the per-keyword notes). design.md is the single source of truth for
   design and reasoning; keep it updated after implementing.

3. **Run the benchmark harness as part of normal PR development, and commit its results.**
   - Run `make benchmark` (see
     [json_schema_rs_benchmark/README.md](json_schema_rs_benchmark/README.md)).
   - The harness is the **guiding star** for performance: it is how we detect and reason
     about performance regressions and improvements across our library and competitors.
   - The committed results files (`json_schema_rs_benchmark/results.json` and
     `json_schema_rs_benchmark/results.md`) **must be up to date** so PR review sees the
     real performance impact of the change in the diff.
   - **A PR is not complete until the benchmark harness has been run and its results files
     are committed and current.** This is non-negotiable.

## Testing and lint standards (summary; contribution-guide is authoritative)

- One assertion per test; named `expected`/`actual` with explicit type annotations; compare
  the **whole** value (full golden strings for codegen), never a subset/`contains`.
- Run `make lint test` before completing any change. Also run `cargo test --features uuid`
  when touching uuid-gated code.
- Use `#[expect(...)]`, never `#[allow(...)]` (workspace forbids `allow_attributes`).
- No literal recursion — use an explicit stack/queue so deep inputs cannot overflow.

## Version control

- **Never run `git add`, `git commit`, or `git push`.** The maintainer handles all staging
  and commits. Make the edits (including refreshing the benchmark results files) and leave
  version control to them.
