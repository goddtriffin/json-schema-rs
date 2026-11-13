# CLAUDE.md — json-schema-rs

Operating rules for working in this repository. These are **hard requirements**, not
suggestions. They exist so every change stays aligned with our standards and our vision.
For consumer-facing information see [README.md](README.md); for design and architecture
see [design.md](design.md).

## Priorities, in order

1. **Spec adherence and correctness.** Behaviour matches the JSON Schema specification for
   every draft we support. Correctness is everything: nothing outranks it.
2. **Performance: speed, then memory.** Fast and lean, measured with `make benchmark`
   (and against competitors with `make benchmark_full`).
   When speed and memory conflict, choose speed: a much faster design is worth somewhat
   more memory.
   When the valid and invalid paths conflict, the **valid instance** wins: most real input is
   valid, and reporting errors legitimately costs more. "Valid" is the instance's final
   verdict, not a keyword's: the branches `anyOf`, `oneOf`, `if`, `not` and `contains` reject
   while validating a valid instance are on the fast path too. An invalid instance may cost
   more, but only by a bounded factor (no super-linear blow-up, no re-evaluating subtrees
   over and over), since invalid input is often untrusted; and `is_valid` keeps its early exit
   cheap.
3. **The user-facing API.** Sane defaults and the behaviour users would most expect (library,
   CLI, generated code). Keep the spec-faithful default; offer a setting where users
   reasonably differ. One deliberate exception, chosen by the maintainer: codegen maps
   `format: "uuid"` to `Uuid` by default even where 2019-09 / 2020-12 only annotate formats
   (`FormatTypes::Typed`; `FollowDialect` is the spec-exact setting; see design.md, *format*).

**Tests adapt to the library, never the reverse.** Never shape the API, defaults or output to
make testing easier, deterministic or idempotent. When the best behaviour is hard to test, the
tests do the work (pin a setting, sort, compare as sets).

Backwards compatibility is **not** a priority and **never** a justification (requirement 4).

## Hard requirements for every code change

1. **Invoke the `contribution-guide` skill before writing or changing any code.** Always.
   It defines the implementation workflow, testing standards, and repository conventions.
   Do not begin an implementation without it.

2. **Refresh your understanding from [design.md](design.md) before implementing.** Read the
   section(s) relevant to what you are about to change (architecture, values, design
   principles, and the per-keyword notes). design.md is the single source of truth for
   design and reasoning; keep it updated after implementing.

3. **Run the benchmark and differential harnesses once, at the end, when the PR changes what
   our consumers run.**
   - **When:** the PR changes anything that can change a consumer's experience or program:
     the library, the CLI or the macros (`json_schema_rs/`, `json_schema_rs_macro/`, outside
     their tests), what they generate, or their dependencies or features. A PR that does not
     (docs, agent instructions, tests only, CI, Makefile targets that do not change what is
     built) skips both harnesses to save time, and its PR body says so in one line under each
     section (`n/a: no library change`). A PR that changes a harness runs that harness (see
     `make benchmark_full` below for the benchmark).
   - Correctness comes first. Only after `make lint test` passes and the change is done, run
     `make benchmark` and `make differential` as the last step before opening (or updating)
     the PR. Each takes several minutes: never run them mid-work or repeatedly.
   - The benchmark harness is the **guiding star** for performance (see
     [json_schema_rs_benchmark/README.md](json_schema_rs_benchmark/README.md)). It has two
     timed modes, and `make benchmark_smoke` (about 25 s) checks every cell without timing,
     safe to run mid-work whenever a change could break a cell:
     - `make benchmark` (**every PR that changes the library**, about 3–13 min): our library
       and CLI only, built from `main` and from the working tree and measured side by side in
       the same session, so run-to-run drift cancels out. It prints the PR body's *Performance* section and changes
       no committed file.
     - `make benchmark_full` (about 60 min): every tool, ours and the competitors', and it
       refreshes the committed `json_schema_rs_benchmark/results.{json,md}`. Run it as well as
       `make benchmark` (which still writes the PR body's report) when the PR changes the benchmark
       harness, a competitor or a fixture, or the derive or reverse codegen (the quick mode
       does not measure reverse codegen), and on a quiet machine when the cross-tool tables
       should be refreshed.
   - The differential harness measures how much of the official JSON Schema Test Suite we pass
     next to other implementations (see
     [json_schema_rs_differential/README.md](json_schema_rs_differential/README.md)). Its
     committed results (`json_schema_rs_differential/results.{json,md}`) **must be up to date**
     so PR review sees the real impact of the change in the diff.
   - **A PR that changes the library is not complete until `make benchmark` and
     `make differential` have been run, the differential results are committed and current,
     and the PR body carries the quick benchmark's report** (plus the full run's refreshed
     results when the PR needs one). This is non-negotiable.

4. **No backwards compatibility before 1.0.** The crate is pre-1.0 (`0.x`): there are no
   users to keep working, so every change makes the API, behaviour and docs what they should
   be, directly. Rename, reshape or remove anything; never keep an old name, alias,
   re-export, redundant accessor, `#[deprecated]` shim, forwarding wrapper, old convention or
   "kept for compatibility" path because it existed before, and never ask whether to keep
   the old behaviour. Backwards compatibility is never a reason for any decision, in code,
   docs, issues or PRs: make the best thing possible. When you notice such a leftover outside your change, file an issue for
   it in its place in the roadmap (#15). This is about *our own* earlier API; what a JSON
   Schema draft requires for schema authors (draft-04 `id`, `dependencies` in 2020-12,
   `definitions`) is spec behaviour and stays.

## Where knowledge goes

Every piece of knowledge has one home. Never record it anywhere else.

- **Something to do** (a bug, a finding, a measured opportunity, a follow-up that cannot land in the current PR) → a **GitHub issue**, with its evidence and numbers.
- **Why something is the way it is** (a decision, a rejected alternative, a trade-off, a spec divergence) → **design.md**, written when the PR that decides it lands.
- **How to work** (process, rules, checklists) → CLAUDE.md and the skills. They hold only advice that stays true, never findings, numbers or backlogs: those drift.
- **What the code does** → the code itself. Docs and comments never restate it.

## Testing and lint standards (summary; contribution-guide is authoritative)

- One assertion per test; named `expected`/`actual` with explicit type annotations; compare
  the **whole** value (full golden strings for codegen), never a subset/`contains`.
- Name tests for the behaviour they check, never for an issue, PR or bug number
  (`integer_accepts_zero_fraction_float`, not `issue_29_integer_...`). A known-failure test
  carries its issue as the `known_failure(N, ...)` argument and its `// --- #N: ... ---` group
  comment, so unwrapping it on the fix leaves a name that stays true.
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
   the final step when the PR changes the library (requirement 3). Only open a PR that is
   ready for a human to review.
3. **Commit, push, and open the PR** once it is ready. Link the issue it closes (`closes #N`).
   Write the body from the [PR template](.github/PULL_REQUEST_TEMPLATE/pull_request_template.md);
   see *Pull request body* below.
4. **Never merge a PR**, and never push to `main`. Wait for a human reviewer to approve and merge.
5. **After the PR is merged:** switch to `main`, pull, delete the local feature branch (and the
   remote one if GitHub did not), and close any related issues that did not close automatically
   (and tick them off in the roadmap #15).

## Dependencies

- **Declared once, at the workspace level.** Every dependency of every crate is in the root
  `Cargo.toml`'s `[workspace.dependencies]` (and, for the isolated benchmark workspace, in
  `json_schema_rs_benchmark/isolated/Cargo.toml`'s), with a full `M.m.p` requirement. Members
  inherit it (`name.workspace = true`) and add only their own `features` and `optional`; a
  member never names a version or path itself.
- **Latest releases.** Add and upgrade dependencies at their latest stable release. Keep an older
  one only when another dependency's API requires it, and say why in a comment on its line.
- **Our crates only.** Upgrades never touch the vendored competitor clones under `research/repos`.

## Clean up after yourself

Never leave anything running or lying around that you are not actively using. The machine is
shared with other sessions, and whatever you leave behind costs them CPU, memory, disk and
benchmark accuracy.

- **Background processes:** stop every background shell, wait loop, monitor, server, benchmark
  or smoke test as soon as you no longer need its result. Before ending a turn, check that
  nothing of yours is still running. A wait loop must be able to exit: `pgrep -f <pattern>`
  matches the loop's own command line, so `until ! pgrep -f ...` never ends; wait on an output
  file or the job's own exit instead.
- **Files:** delete scratch files, copied binaries, temporary crates and their target directories
  once you are done with them. Run `cargo clean` after build-heavy work when the disk is tight.
- **Git:** once a PR is merged, clean up its branch (see *Version control*, step 5) and any
  worktree you created for it.

## Pull request body

Succinct, at-a-glance, actionable: what changed and why, then these two sections, both
measured against `main`. Highlight what matters most and give
counts for the rest in the visible text; full detail goes only in collapsed `<details>`
blocks (the Performance tables), and nothing the PR did not cause appears at all.

- **JSON Schema spec parity:** official-suite tests that failed on `main` and pass now (and
  any that regressed). Sources: entries removed from (or added to)
  `json_schema_rs/tests/suite_xfail.json` (and `suite_xfail_arbitrary_precision.json` for the `arbitrary-precision` build), and the compliance/coverage change per draft in
  `json_schema_rs_differential/results.md`. Name the most relevant groups (keyword and
  draft) with counts. If nothing changed, write `n/a`.
- **Performance:** our library's time and memory (peak heap / peak RSS) versus `main`, from
  `make benchmark`'s report (`target/benchmark-quick/report.md`), which measures both in the
  same session. A cell counts only if it moved beyond noise (the report's rule: the median of each
  side's per-round minimums differs by more than 3% and more than the two sides' spreads) **and** sits on a code path the PR
  changed; a cell that moved but runs only code the PR did not touch is noise and is left
  out. When the PR also ran `make benchmark_full`, say how category winners changed. Write a short visible summary (the biggest few with before → after, the count of
  cells that moved, any change in category winners, and why things moved), then two tables,
  **Memory** and **Time**, each collapsed by default in its own `<details>` block, listing
  **every** counted cell: tier, fixture, operation, `main`, this PR, change (%), sorted by
  change. Leave a blank line after `<summary>` so the Markdown table renders. If nothing moved
  beyond noise, say so in one line and omit the tables. Then the report's **Generated code**
  part (#113): the generated code's size per fixture is exact, so every change counts. Give its
  summary line (total lines and bytes, before → after) and why the output changed, with its
  table collapsed in a `<details>` block; if nothing changed, its one line.
