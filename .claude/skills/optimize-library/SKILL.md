---
name: optimize-library
description: Use when asked to optimize json-schema-rs ("optimize the library", make it faster, use less memory, generate less code, beat a competitor's numbers). Finds the biggest measured win, implements it, and opens one PR per improvement. Holds the open optimization findings, the data-oriented lenses and the measurement lessons learned so far.
---

# Optimize the library

Optimization is open-ended, so it lives here instead of in issues. Each run of this skill ships **one measured improvement as one PR**, then waits for the maintainer to merge it before starting the next.

Follow the `contribution-guide` skill and CLAUDE.md as for any change. This skill adds what is specific to performance work.

## Priorities

1. **Correctness is a constraint, not a trade-off.** Same verdicts, same errors, same generated behaviour, same suite and differential results. A faster wrong answer is a bug.
2. **Speed**, then **memory**, then **generated code size**. When two conflict, the earlier one wins: a much faster design is worth somewhat more memory.
3. **No lost functionality.** A trade against a higher priority needs the benchmark to show its cost is negligible.

## Workflow

1. **Pick a target**, biggest expected win first:
   - **Competitor gaps:** `json_schema_rs_benchmark/results.md` (*Category winners* and the per-operation tables). Every cell a Rust competitor wins is a target.
   - **Generated code:** the *Generated code* tables in the same file. Look at lines and bytes per category for the biggest fixtures.
   - **Known findings:** the list below.
   - **A profile** of the hot path: macOS `sample <pid>`, `cargo build -Ztime-passes` for build time, or a counting `GlobalAlloc` probe for heap.
2. **Measure before changing anything.** Re-measure the finding on today's `main`: numbers older than a few PRs are often wrong. Write a scratch probe in the scratchpad, with its own `CARGO_TARGET_DIR`, depending on the crate by path. Measure the **upper bound** of the idea (the result with the work removed entirely) before building it. If the bound is small, pick another target.
3. **Implement the smallest change that captures the win.** Prefer redesigning the data layout or the algorithm over micro-tuning.
4. **Verify**, in this order:
   - `make lint test`;
   - `make benchmark_smoke`, if a benchmark cell could break;
   - `make benchmark` and `make differential`, once, last.
5. **Open the PR** with the *Performance* section from `target/benchmark-quick/report.md` (CLAUDE.md, *Pull request body*), then stop until it is merged.
6. **Update this skill** in the same PR:
   - remove a finding the PR resolves;
   - add any new finding it uncovered, with numbers and date;
   - add a lesson if one was learned.

File an issue only for a finding that cannot land in the current PR (it needs a design decision, or is too big for one PR). Never file tracker issues.

## Correctness rules for fast paths

- A fast path must give **exactly** the general path's result.
- **Validator:** verdict, errors and their order, annotations for `unevaluated*`. When the fast path cannot decide, or fails, fall back to the general path and let it build the errors. The discriminated `anyOf` / `oneOf` (#65) is the pattern to copy.
- **Codegen:** output byte-identical, or golden tests updated with the generated code still accepting every schema-valid instance.
- The suite gate checks on every case that `is_valid` and `validate` agree; keep both modes covered.
- No unbounded recursion: use explicit stacks.

## Measurement lessons

- **Premises go stale.** #57 targeted registry location storage, but it measured 1–55 KB; the real cost was copying loaded documents (−45% heap). #65 targeted serde tagging, but variants were already chosen by validation; the win was in the validator. Measure where the time and memory actually go.
- **Check worst-case schema shapes, not just the target.** A compile-time pass that looked free cost data-warehouse +32% compile time, because of one `anyOf` of 220 `$ref`s that all share an `enum`. Bail out early and record candidates while compiling instead of rescanning.
- **One-time costs are cheaper than per-call costs**, but they still count: compile time is a benchmark cell.
- **A pattern the fixtures lack is invisible to `make benchmark`.** Show the win with a probe, then add a real-world fixture (as `medium/geojson` was for tagged unions). A fixture change normally needs `make benchmark_full`; ask the maintainer whether to run it.
- **Noise rule:** a cell counts only when it moved by more than 3% and more than the two sides' spreads, **and** sits on a code path the PR changed.
- **Heap artifacts:** peak heap can move from `String` / `Vec` capacity doubling alone. Before reporting a memory change, compare both sides with a pre-sized buffer (a +3.8% codegen "regression" in #141 was this).
- **Codegen build time** of generated code is dominated by generic instantiations: count them with `-Ztime-passes` or the LLVM IR size, not by reading the source.
- **Wait loops** must wait on the job's own exit file, never on a log left over from a previous run.

## Data-oriented lenses

Apply each one to every subsystem: parser and schema model, `$ref` registry, compiled validator and evaluator, type graph and Rust codegen, derive and reverse codegen, CLI.

1. > how do we rewrite this to better leverage data-oriented design principles
2. > can you replace those hash tables with a flat array? can we make the pointers an index into the array? can we pack the most commonly used data into upper bits of the index?
3. > caching is cope for a slow design. do not add caches and delete all existing caches. instead, redesign it to be faster
4. > should this be a structure of arrays instead of an array of structures?
5. > replace the heap-allocated lists with a stack allocated one with an inline capacity
6. > do not use an array of bools. Use a bitset instead
7. > can we use arenas and do stack allocated cleanup for things we bulk free
8. > You need to be thinking bigger. 1% faster is not enough. This should be 10x faster.

## Known findings

Each finding carries its date. Re-measure it first.

**Validator speed** (results.md of 2026-11-05):
- The `jsonschema` crate is 1.3–2.5× faster in-process on 11 of 12 fixtures. Worst: package-json, k8s, circleci, data-warehouse. Roadmap issue #143 sets the target.
- Generated-type unions still evaluate variant by variant (`TypeValidators::read_union`), so they don't use the discriminator. Roadmap issue #128.

**Codegen time and memory** (sample profile, 2026-11-03):
- About a third of `generate` is embedding the validator (`BufferValidators::render` → `embedding`). It compiles the entries with locations (`compile_located`), prunes the documents, then compiles the pruned documents again to prove `same_validation`.
- Candidates:
  - derive the proof structurally instead of compiling a second time;
  - share one compile between the two passes;
  - skip the proof when pruning removed nothing.
- data-warehouse timings then: 35.7 ms with the defaults; 29.3 ms without newtypes; 23.0 ms without self-validation; 8.0 ms with neither.

**Build time of generated code** (2026-11-07):
- `generated/wide-object` takes 87 s to build with the benchmark profile.
- About 68% of its LLVM IR is `__private::read_field::<T>`, instantiated for 1,287 distinct newtypes × 2 input formats (`from_str`, `from_value`).
- Fix: fewer distinct newtype types, or one shared reader per inner type.

**Generated code size** (2026-11-12):
- Totals over all fixtures after #141 (before `medium/geojson` was added): 105,891 lines, 5.89 MB.
- Biggest categories:
  - the embedded schema (data-warehouse 441 KB of 1.9 MB; package-json 820 KB of 2.8 MB);
  - attributes (data-warehouse 666 KB before #141);
  - docs (legitimate: schema descriptions).
- Candidates:
  - shorter derive and attribute lines (full `json_schema_rs_macro::` paths repeated per type, about 201 KB in data-warehouse);
  - `skip_serializing_if` (about 55 KB);
  - embedded-schema pruning and sharing (entry URI lists repeat the base URI per type);
  - per-type boilerplate a library macro could replace;
  - structurally identical types emitted twice;
  - derives a type does not need.
- Done so far:
  - #137 `self_validating!` macro: −32% lines;
  - #138 fragment entries: −2.9% bytes;
  - #141 `rename_all`: −1.7% bytes.
- Renames left after #141 are values like `team_9` that no serde rule produces.

**Codegen buffers** (2026-11-12):
- `render_buffer` copies the body into a second `Vec<u8>`. Inserting the header in place did not change the peak heap; only worth revisiting together with another output change.
