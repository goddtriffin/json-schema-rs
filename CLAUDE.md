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

3. **Run the benchmark and differential harnesses once, at the end, and commit their results.**
   - Correctness comes first. Only after `make lint test` passes and the change is done, run
     `make benchmark` and `make differential` as the last step before opening (or updating)
     the PR. Each takes several minutes: never run them mid-work or repeatedly.
   - The benchmark harness is the **guiding star** for performance: it is how we detect and
     reason about performance regressions and improvements across our library and
     competitors (see [json_schema_rs_benchmark/README.md](json_schema_rs_benchmark/README.md)).
     The differential harness measures how much of the official JSON Schema Test Suite we pass
     next to other implementations (see
     [json_schema_rs_differential/README.md](json_schema_rs_differential/README.md)).
   - The committed results files (`json_schema_rs_benchmark/results.{json,md}` and
     `json_schema_rs_differential/results.{json,md}`) **must be up to date** so PR review sees
     the real impact of the change in the diff.
   - **A PR is not complete until both harnesses have been run and their results files are
     committed and current.** This is non-negotiable.

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
2. **Review your own changes before opening the PR.** Read the full diff, run `make lint test`,
   and fix what you find. Once it is correct, run `make benchmark` and `make differential` as
   the final step (requirement 3). Only open a PR that is ready for a human to review.
3. **Commit, push, and open the PR** once it is ready. Link the issue it closes (`closes #N`).
   Write the body from the [PR template](.github/PULL_REQUEST_TEMPLATE/pull_request_template.md);
   see *Pull request body* below.
4. **Never merge a PR**, and never push to `main`. Wait for a human reviewer to approve and merge.
5. **After the PR is merged:** switch to `main`, pull, delete the local feature branch, and close
   any related issues that did not close automatically (and tick them off in the roadmap #15).

## Pull request body

Succinct, at-a-glance, actionable: what changed and why, then these two sections, both
measured against `main`'s committed results files. Highlight what matters most and give
counts for the rest; never list everything.

- **JSON Schema spec parity:** official-suite tests that failed on `main` and pass now (and
  any that regressed). Sources: entries removed from (or added to)
  `json_schema_rs/tests/suite_xfail.json`, and the compliance/coverage change per draft in
  `json_schema_rs_differential/results.md`. Name the most relevant groups (keyword and
  draft) with counts. If nothing changed, write `n/a`.
- **Performance:** our library's time and memory (peak heap / peak RSS) versus `main`'s
  benchmark results. Report only changes beyond noise (larger than the 3% threshold and
  the standard deviations the harness uses for winners): the biggest few with before → after,
  plus a count of the rest, and note any change in category winners. If nothing moved
  beyond noise, say so in one line.
