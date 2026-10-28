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
- No *unbounded* recursion. Graph algorithms and tree walks (reference crawling, cycle detection,
  dedup, emission order, `Drop`, rendering) use an explicit stack/queue so deep inputs cannot
  overflow. The one exception is the validator's evaluator, which recurses with a depth limit that
  returns a typed error before the native stack can overflow (see design.md).

## Version control

Every change goes through a pull request that a human reviews and merges.

1. **Work on a feature branch**, never on `main` (e.g. `feat/<topic>`, `fix/<topic>`).
2. **Review your own changes before opening the PR.** Read the full diff, run `make lint test`
   and `make benchmark`, and fix what you find. Only open a PR that is ready for a human to
   review.
3. **Commit, push, and open the PR** once it is ready. Link the issue it closes (`closes #N`).
4. **Never merge a PR**, and never push to `main`. Wait for a human reviewer to approve and merge.
5. **After the PR is merged:** switch to `main`, pull, delete the local feature branch, and close
   any related issues that did not close automatically (and tick them off in the roadmap #15).
