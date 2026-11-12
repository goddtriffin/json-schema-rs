---
name: optimize-library
description: Use when asked to optimize json-schema-rs ("optimize the library", make it faster, use less memory, generate less code, beat a competitor's numbers). Measures to find the biggest win, implements it, and opens one PR per improvement.
---

# Optimize the library

Each run ships **one measured improvement as one PR**, then waits for the maintainer to merge it before starting the next. Targets are found by measuring `main` every time, never from a stored list: recorded findings drift.

Follow the `contribution-guide` skill and CLAUDE.md as for any change. This skill adds what is specific to performance work.

## Priorities

1. **Correctness is a constraint, not a trade-off.** Same verdicts, same errors, same generated behaviour, same suite and differential results. A faster wrong answer is a bug.
2. **Speed**, then **memory**, then **generated code size**. When two conflict, the earlier one wins: a much faster design is worth somewhat more memory.
3. **No lost functionality.** A trade against a higher priority needs the benchmark to show its cost is negligible.

## Workflow

1. **Find the target** with the biggest expected win:
   - **Open performance issues** first: they hold findings already measured. Re-measure before acting.
   - **Competitor gaps:** `json_schema_rs_benchmark/results.md` (*Category winners* and the per-operation tables). Every cell a Rust competitor wins is a target.
   - **Generated code:** the *Generated code* tables in the same file, by category, for the biggest fixtures.
   - **Profiles** of the hot paths: macOS `sample <pid>` for time, a counting `GlobalAlloc` probe for heap, `cargo build -Ztime-passes` or the LLVM IR size for the build time of generated code.
2. **Measure the idea's upper bound before building it:** the result with the work removed entirely, in a scratch probe (scratchpad, own `CARGO_TARGET_DIR`, the crate by path). If the bound is small, pick another target.
3. **Implement the smallest change that captures the win.** Prefer redesigning the data layout or the algorithm over micro-tuning.
4. **Verify**, in this order:
   - `make lint test`;
   - `make benchmark_smoke`, if a benchmark cell could break;
   - `make benchmark` and `make differential`, once, last.
5. **Open the PR** with the *Performance* section from `target/benchmark-quick/report.md` (CLAUDE.md, *Pull request body*), then stop until it is merged.

Where what you learn goes (CLAUDE.md, *Where knowledge goes*):
- **A finding that will not land in this PR** (it needs a design decision, is too big, or is another target): a GitHub issue with its numbers. Never tracker issues.
- **Why the PR chose its design, or why an idea was rejected:** design.md, in the same PR.
- **This skill:** only advice that stays true (a rule, a lens, a place to look). Never findings or numbers.

## Correctness rules for fast paths

- A fast path must give **exactly** the general path's result.
- **Validator:** verdict, errors and their order, annotations for `unevaluated*`. When the fast path cannot decide, or fails, fall back to the general path and let it build the errors. The discriminated `anyOf` / `oneOf` dispatch in `validator/eval.rs` is the pattern to copy.
- **Codegen:** output byte-identical, or golden tests updated with the generated code still accepting every schema-valid instance.
- The suite gate checks on every case that `is_valid` and `validate` agree; keep both modes covered.
- No unbounded recursion: use explicit stacks.

## Measurement rules

- **Measure where the cost is before choosing the fix.** An issue's premise is a hypothesis: the time or memory is often somewhere else, or in another layer.
- **Check worst-case schema shapes, not just the target.** A pass that is free on the fixture you aimed at can be quadratic on another (a union of hundreds of `$ref` branches, a very wide object, deep nesting). Bail out early, and collect candidates while compiling instead of rescanning.
- **One-time costs count:** schema compile and codegen are benchmark cells too.
- **A pattern the fixtures lack is invisible to `make benchmark`.** Show the win with a probe, then add a real-world fixture for it. A fixture change normally needs `make benchmark_full`; ask the maintainer whether to run it.
- **Noise rule:** a cell counts only when it moved by more than 3% and more than the two sides' spreads, **and** sits on a code path the PR changed.
- **Heap artifacts:** peak heap can move from `String` / `Vec` capacity doubling alone. Before reporting a memory change, compare both sides with a pre-sized buffer.
- **Build time of generated code** is driven by generic instantiations (one copy per distinct type), not by source size.
- **Wait loops** must wait on the job's own exit file, never on a log left over from a previous run.

## Where generated code grows

Look for these in the *Generated code* tables:
- repeated per-type boilerplate a library macro could replace;
- structurally identical types emitted more than once;
- the embedded self-validation schema (pruning, sharing, repeated URIs);
- long attribute and derive lines (full paths, per-member attributes a container attribute could replace);
- derives a type does not need.

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
