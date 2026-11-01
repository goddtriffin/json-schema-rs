---
name: contribution-guide
description: Use when contributing to the json-schema-rs crate (adding features, fixing bugs, understanding code layout, researching specs and competitors). For user-facing information (what the crate supports, how to use it), see the [README](README.md). For design and architecture, see [design.md](design.md).
---

# json-schema-rs (contribution guide)

Supported and unsupported features are documented in the [README](README.md). This skill focuses on **how to contribute**—implementation workflow and resources. Design, architecture, values, and per-feature implementation notes live in [design.md](design.md) at the repo root.

## Purpose

The json-schema-rs crate provides **three tools**:

1. **JSON Schema → Rust struct** (codegen): generate Rust types from a JSON Schema.
2. **Rust struct → JSON Schema** (reverse codegen): generate a JSON Schema from Rust types.
3. **JSON Schema validator**: two inputs—JSON Schema definition and JSON instance—output validation result.

**For every feature we develop, update, or fix, implement it for each of these three tools** where the feature applies (some features may apply to only one or two tools).

## Resources

When implementing a feature, use these resources. Each is described by **what it is**, **how to use it**, and **when to use it**.

- **JSON Schema specs** (`specs/`) — *What:* Local copy of JSON Schema drafts (draft-00 through 2020-12), obtained by running `make vendor_specs` (or `./specs/download.sh`). They are gitignored and not in the repo. *How:* Read HTML/JSON under `specs/json-schema.org/`; run `./research/scripts/list_keywords.sh` from repo root for the canonical keyword list. Use **only** these local specs—no web. If specs are missing, run `make vendor_specs`. *When:* At the start of every feature (understand how the keyword is defined and how it behaves in each draft); when documenting spec-version quirks in design.md.

- **Research reports** (`research/reports/<lang>/{org}-{repo}.md`) — *What:* Structured reports on competitor libraries (what they support, how they implement features). *How:* Read the report for the feature you're implementing; rank approaches by our values (see design.md); add or update sections when you learn something new. *When:* When implementing a feature (see how others did it before writing code); after implementing (contribute back so the report stays the knowledge source). See the **competitor-json-schema-codegen-analysis** skill for how reports are produced and the report template.

- **Cloned competitor repos** (`research/repos/<lang>/<name>/`) — *What:* Git-ignored vendored clones of competitor codebases. *How:* Read source code when the research report lacks detail. Do not run untrusted code without review. *When:* After reading the research report, when you need code-level or high-granularity detail. Prefer the report first; use clones for depth.

- **design.md** (repo root) — *What:* Design and architecture knowledge bank: high-level architecture, values, design principles, per-keyword implementation notes, spec-by-version behavior, version quirks. *How:* Read the relevant section(s) before implementing a keyword; after implementing, update "Our implementation" and "Spec version quirks" (and related subsections) in design.md. Code remains source of truth for literal behavior; design.md stores rules and reasoning. *When:* At the start of a feature (understand our design or that it's TODO); at the end (capture high-level decisions, edge-case rules, spec-version differences).

- **README.md** (repo root) — *What:* Consumer-facing description: what the library does, what each tool supports, which specs we adhere to, how to run it. *How:* Update when you add/change a user-visible feature or spec support. Keep succinct and maximally insightful for developers evaluating or using the library. *When:* When changing supported features, CLI behavior, or anything users see. Do not put design/architecture detail here—point to design.md.

## Implementation workflow

### Before implementing

**Always read the relevant parts of these resources** before implementing a new feature, so you're aware of everything you need to be aware of:

- **design.md** — Our design and implementation notes for the keyword/feature; what's already implemented vs TODO; spec version quirks.
- **README.md** — What we currently expose to users; which specs we support; how the library is presented.
- **JSON Schema specs** (`specs/`) — How the keyword is defined and how it behaves in each draft we care about. Download with `make vendor_specs`; they are gitignored.
- **Competitor research reports** (`research/reports/<lang>/{org}-{repo}.md`) — How other libraries implement this feature; rank by our values (design.md).
- **Locally cloned competitor repos** (`research/repos/<lang>/<name>/`) — When the research report lacks detail, read the source for code-level insight.

Read as much as needed from each; don't skip this step. It ensures we stay spec-aligned, avoid duplicating work, and learn from competitors.

### Implement

Schema model, codegen/validation behavior, tests, examples. Follow Contribution Guidelines below.

### After implementing

1. **Update design.md** — Keep track of which features we've implemented. Add or update the relevant section(s) with our implementation notes, spec version quirks, and any implementation status or keyword summary so design.md stays the single source of truth.
2. **Update the research report** — If you learned something new about a competitor (from a cloned repo or elsewhere) that isn't already covered by the research report, add or update the report so we retain that knowledge for next time.
3. **Update README** — If the change is user-visible (new feature, new keyword support), update the README (see Resources: README.md).

### When implementing a new JSON Schema feature or updating/verifying an existing one

Follow these steps for **every** new JSON Schema feature and for **every** update or verification of an existing feature (e.g. a keyword or related behavior like `type: "object"`):

1. **Spec audit:** Check the keyword (and related behavior) against **all** JSON Schema specifications we support (draft-00 through 2020-12) using **only** the local specs under `specs/` (`make vendor_specs`). Document how the keyword is defined and how it behaves in each relevant draft.
2. **design.md:** Fill out the feature's section in design.md: "Our implementation" and **"Spec version quirks"** (and any subsections, e.g. for `type` and `type: "object"`). Note any differences between spec versions.
3. **Dialect and settings:** If spec versions handle the feature differently, put the difference in the **`Dialect`** (`json_schema/dialect.rs`): a keyword in each draft's keyword table (with its subschema position), and a behaviour as a flag with each draft's default in `Dialect::for_spec_version`. Code reads `schema.dialect()`; never branch on `SpecVersion`. When the flag gets its first consumer, add its per-flag override to `JsonSchemaSettings` (an `Option`, `None` = follow the draft; an enum when the setting is a choice of modes) and apply it in `resolve_dialect`. Shape differences that downstream code should not see (draft-4 `id`) are normalised once at parse time. See design.md, *Dialects*.
4. **Default = latest spec:** With no `$schema` and no settings, the dialect is Draft 2020-12 (the latest we support); `JsonSchemaSettings::default()` pins nothing and follows each document's `$schema`.
5. **All tools:** Implement or update the feature for **every applicable tool**: JSON Schema → Rust codegen (and all frontends: library golden, CLI, generated Rust build + deserialize, macro), and the **validator**. Add tests for each; update the codegen scenario × frontend matrix in design.md when adding scenarios or frontends.

**Checklist** (copy when starting a new feature or verification):

- [ ] Spec audit (local specs only; draft-00 through 2020-12)
- [ ] design.md: Our implementation + Spec version quirks
- [ ] Dialect table/flag (+ settings override) if spec versions differ
- [ ] Default dialect = Draft 2020-12
- [ ] All tools: codegen (all frontends) + validator; tests; scenario × frontend matrix

## Working style

These apply to **every** change, not only JSON Schema features.

### Prefer `make` targets over hand-written commands

Always drive repo workflows through their **Makefile target** rather than a manually crafted command, wherever a target exists: `make test` (not a bare `cargo test`), `make lint`, `make benchmark`, `make differential`, `make vendor_specs`, `make vendor_test_suite`, and so on. The targets encode the correct flags and stay correct as the project evolves — for example, `make test` runs with `--all-features` so feature-gated code (like the `uuid` feature) is always compiled and exercised, whereas a raw `cargo test` silently skips optional features and drifts from what reviewers and CI run. Reach for a raw `cargo`/shell command only when no target covers the task (e.g. a one-off `cargo test <name>` while iterating on a single test); when a raw workflow recurs, add or extend a `make` target instead.

### Check validator changes against other implementations

Every PR runs `make differential` once, as the final step together with `make benchmark` (after correctness is confirmed), and commits the refreshed `json_schema_rs_differential/results.{json,md}`. It runs our validator next to other implementations through Bowtie, so the diff shows changes in coverage (unsupported keywords), correctness, and disagreements with the majority; the PR body's *JSON Schema spec parity* section summarises that change (see CLAUDE.md, *Pull request body*). See `json_schema_rs_differential/README.md` for prerequisites (bowtie, Docker, `make vendor_test_suite`).

### Orchestrate with subagents

Default to **fanning work out to subagents** and keeping the main agent as an **orchestrator**. This serves two goals: it keeps large reads, searches, and iterative compile/test loops out of the main context (saving context budget), and it **parallelizes** independent implementation work. Decompose a change into independent units (e.g. distinct tools, files, or scenarios), dispatch each to a subagent with a precise, self-contained brief, and have the orchestrator integrate and verify the results. Give each subagent a **narrow, non-overlapping** scope so they never edit the same files concurrently. Implement directly (no fan-out) only for small, tightly-coupled changes where coordination overhead would exceed the benefit.

### Verify at the end of a round — never mid-flight

Run `make test` to confirm correctness at the **end of an implementation round**, once the code is actually ready for linting and testing. **Do not** run `make test`/`make lint`/`make benchmark` in the *middle* of development — and especially not while subagents are still working. Mid-round verification at best slows the round down, and at worst breaks a subagent's in-flight work because that code was not yet ready to compile, lint, or test. The orchestrator runs the full verification pass **once**, after all subagents have returned and their work is integrated. `make benchmark` and `make differential` take several minutes each: run them only once the change is correct, as the very last step before opening or updating the PR, never in the middle of work. (A subagent may run a **narrow, targeted** check on its own isolated piece before returning; the *full* `make test` / `make benchmark` sweep is the orchestrator's job at the end.)

## Contribution Guidelines

### Git

- **Branch, PR, never merge.** Work on a feature branch, review your own diff, then commit, push and open a PR once it is ready for human review. Never merge a PR or push to `main`; a human reviewer merges. After the merge, reset the local checkout to `main` and close any related issues that did not close automatically. See [CLAUDE.md](../../../CLAUDE.md), *Version control*.

### Testing

- **Inlined tests**: Most tests have input and expected output inlined in the test method (no file loading).
- **File-based test**: When the test suite includes file-based tests (schema + expected output), **always update those files** when implementing new features so the file-based test exercises the new behavior.
- **Unit tests**: Add `#[cfg(test)]` tests in the relevant module(s) for feature-specific logic. **Always write exhaustive unit tests** so that every new feature is fully verified. At a minimum, have unit tests that cover **success and failure conditions**, and **edge cases** that you are aware of. Aim for one unit test per **code path** (e.g. one test for each possible outcome or branch), plus **opposite pairings** such as success vs failure, enabled vs disabled, bounds present vs absent, or fallback vs non-fallback.
- **Test shape**: Prefer **one assertion per test**. Each test should have an `expected` value, an `actual` value, and a single comparison (e.g. `assert_eq!(expected, actual)`). Test the **whole scenario**: avoid asserting on subsets of `actual` (e.g. no `actual.contains(...)` or checking only one field); compare the full value so the test validates the entire behavior. **Exceptions**: When the type does not support `PartialEq` (e.g. some error types), use a single `assert!(matches!(actual, ...))` with a named `actual`; document the expected variant in a comment if helpful.
- **Test names describe behaviour**: name a test for what it checks (`integer_accepts_zero_fraction_float`), never for an issue, PR or bug number (`issue_29_...`). A test outlives the issue that prompted it; the link lives in the `known_failure(N, ...)` argument and the file's `// --- #N: ... ---` group comment, both of which go away when the bug is fixed.
- **Assertions**: Always use named `expected` and `actual` and a single comparison; for string output use full expected strings and `assert_eq!(expected, actual)`.
- **Integration tests**: Integration tests use the public API; keep them in the integration test module.
- **Official suite gate and known failures**: `make test` runs the official JSON Schema Test Suite against the known-failure manifest `json_schema_rs/tests/suite_xfail.json` and fails on unexpected failures **and** unexpected passes. When a change makes suite tests pass, remove their entries (or run `make update_suite_xfail` and review the diff). Every open bug has a test wrapped in `known_failure` / `known_failure_in_child_process` (`tests/known_failures.rs` in each crate); when fixing a bug, unwrap its tests into plain `#[test]`s. When filing a bug you do not fix in the same change, add such a test. See design.md, *Official JSON Schema Test Suite* and *Known failures*.
- **Codegen scenario × frontend coverage**: For **codegen** features and edge cases, treat each as a **scenario** (e.g. "single required string", "nested object", "hyphenated key", "dedupe", "model name source"). **Requirement:** Implement the same scenario (same schema(s) and expectations) across every **applicable** codegen frontend: **Golden string** (library API: unit in `rust_backend.rs` and/or integration in `integration.rs` with full expected string); **CLI** (run `jsonschemars generate rust -o DIR ...`, assert output files and content); **Generated Rust build + deserialize** (temp Cargo crate, `cargo build`, `cargo run` with deserialization); **Macro** (single inline, single path, multiple inline, multiple path as applicable). When adding a **new scenario**, add tests for all applicable frontends. When adding a **new frontend**, add tests for all existing scenarios that apply. Document the scenario and frontend coverage (e.g. in design.md or a testing doc) so the matrix stays up to date. This ensures codegen behavior stays in lockstep for every consumer entry point.
- **Large struct construction in tests:** When a test creates an instance of a large struct (e.g. `JsonSchema`), only manually set the fields that are relevant to the test. Use `..Default::default()` to fill in the rest. This makes it clear which fields and behavior the test cares about.

### Code Conventions

- **Priorities, in order:** (1) spec adherence and correctness, which outranks everything; (2) performance, speed first, then memory: when they conflict, take the much faster design even if it uses somewhat more memory; (3) the user-facing API: sane defaults and the behaviour users would most expect, with a setting where users reasonably differ. Tests adapt to the library, never the reverse: never shape the API, defaults or output to make testing easier or deterministic. See [CLAUDE.md](../../../CLAUDE.md), *Priorities*.
- **No backwards compatibility before 1.0:** while the crate is `0.x`, change the API, behaviour and docs to what they should be directly. Never keep old names, aliases, re-exports, redundant accessors, `#[deprecated]` shims, forwarding wrappers or old conventions for compatibility, and do not ask whether to. Backwards compatibility is never a justification for a decision. Leftovers found outside your change get an issue in the roadmap (#15). Spec-mandated compatibility for schema authors (draft-04 `id`, `dependencies` in 2020-12) is not this and stays. See [CLAUDE.md](../../../CLAUDE.md), requirement 4.
- **Durable wording only:** Never use temporal or minimal phrasing in docs or comments. Do not say "first pass," "in this pass," "keywords supported in this pass," "not yet implemented," "not yet feature complete," or "in this release." Document what is implemented and what is ignored; avoid framing by release or phase.
- **Rust conventions and style:** We care greatly that our code adheres to **idiomatic Rust**—standard types, traits, patterns, and the [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/). We often review competitor libraries implemented in other languages; our library is written in Rust and **must** follow Rust conventions, coding style, and best practices (e.g. use `Result` for fallible operations, prefer enums over stringly-typed errors). Do not mirror non-Rust idioms from competitors when they conflict with Rust conventions. This applies to production code and tests.
- Run `make lint test` to verify — at the **end of an implementation round**, never mid-development; `make benchmark` and `make differential` once, last, before the PR. See [Verify at the end of a round](#working-style).
- Use `#[expect]` not `#[allow]` for Clippy overrides.
- Never fail silently; log errors internally (customer-facing message can differ).
- Follow existing patterns: custom Error enum, BTreeMap for ordering, explicit type annotations on all variables.
- **No unbounded recursion:** Prefer an iterative approach (explicit stack or queue) instead of recursive calls so the crate is not vulnerable to stack overflow on deep inputs. If recursion is needed conceptually, implement it with a loop and a stack. The one exception is the validator's evaluator, which recurses with a depth limit that returns a typed error before the stack can overflow (see design.md, *No unbounded recursion*).
- **Todo list hygiene:** When an idea from `todo.txt` is implemented, remove that entry from `todo.txt` after the implementation is complete.

### Adding New JSON Schema Support

- Add schema model fields only when needed; use `#[serde(default)]` and `Option` so extra keys in the JSON are ignored.
- For unsupported types, decide project policy (ignore vs fail); document in design.md or README as appropriate.

## Repository layout

- **Workspace**: Single crate `json_schema_rs/` (library and `jsonschemars` CLI binary). Root `Cargo.toml` defines the workspace only.
- **JSON Schema specs** (downloaded via `make vendor_specs`, gitignored): `specs/`
- **Competitor clones**: `research/repos/<lang>/<name>/`
- **Research reports**: `research/reports/<lang>/{org}-{repo}.md`
- **Design and architecture**: `design.md` (repo root)

Key source files are in `json_schema_rs/src/` (e.g. `json_schema.rs`, `code_gen.rs`, `code_gen_rust_backend.rs`, `code_gen_settings.rs`, `error.rs`). CLI and build commands are documented in the README.
