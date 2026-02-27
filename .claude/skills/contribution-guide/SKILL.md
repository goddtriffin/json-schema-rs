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
3. **Settings and SpecVersion:** If different spec versions handle the feature differently, capture that in **settings** (e.g. options under `JsonSchemaSettings` or validator/codegen settings). Ensure the **SpecVersion** enum's `default_schema_settings()` (and any other version-driven API) returns the correct settings for each spec version.
4. **Default = latest spec:** Ensure default settings (e.g. `JsonSchemaSettings::default()`) target the **latest** JSON Schema specification we support (Draft 2020-12) unless a different spec version is explicitly provided.
5. **All tools:** Implement or update the feature for **every applicable tool**: JSON Schema → Rust codegen (and all frontends: library golden, CLI, generated Rust build + deserialize, macro), and the **validator**. Add tests for each; update the codegen scenario × frontend matrix in design.md when adding scenarios or frontends.

**Checklist** (copy when starting a new feature or verification):

- [ ] Spec audit (local specs only; draft-00 through 2020-12)
- [ ] design.md: Our implementation + Spec version quirks
- [ ] Settings/SpecVersion if spec versions differ
- [ ] Default settings = Draft 2020-12
- [ ] All tools: codegen (all frontends) + validator; tests; scenario × frontend matrix

## Contribution Guidelines

### Git

- **Never run `git add`, `git commit`, or `git push`.** The maintainer will always handle version control themselves. Make edits and leave staging and commits to them.

### Testing

- **Inlined tests**: Most tests have input and expected output inlined in the test method (no file loading).
- **File-based test**: When the test suite includes file-based tests (schema + expected output), **always update those files** when implementing new features so the file-based test exercises the new behavior.
- **Unit tests**: Add `#[cfg(test)]` tests in the relevant module(s) for feature-specific logic. **Always write exhaustive unit tests** so that every new feature is fully verified. At a minimum, have unit tests that cover **success and failure conditions**, and **edge cases** that you are aware of. Aim for one unit test per **code path** (e.g. one test for each possible outcome or branch), plus **opposite pairings** such as success vs failure, enabled vs disabled, bounds present vs absent, or fallback vs non-fallback.
- **Test shape**: Prefer **one assertion per test**. Each test should have an `expected` value, an `actual` value, and a single comparison (e.g. `assert_eq!(expected, actual)`). Test the **whole scenario**: avoid asserting on subsets of `actual` (e.g. no `actual.contains(...)` or checking only one field); compare the full value so the test validates the entire behavior. **Exceptions**: When the type does not support `PartialEq` (e.g. some error types), use a single `assert!(matches!(actual, ...))` with a named `actual`; document the expected variant in a comment if helpful.
- **Assertions**: Always use named `expected` and `actual` and a single comparison; for string output use full expected strings and `assert_eq!(expected, actual)`.
- **Integration tests**: Integration tests use the public API; keep them in the integration test module.
- **Codegen scenario × frontend coverage**: For **codegen** features and edge cases, treat each as a **scenario** (e.g. "single required string", "nested object", "hyphenated key", "dedupe", "model name source"). **Requirement:** Implement the same scenario (same schema(s) and expectations) across every **applicable** codegen frontend: **Golden string** (library API: unit in `rust_backend.rs` and/or integration in `integration.rs` with full expected string); **CLI** (run `jsonschemars generate rust -o DIR ...`, assert output files and content); **Generated Rust build + deserialize** (temp Cargo crate, `cargo build`, `cargo run` with deserialization); **Macro** (single inline, single path, multiple inline, multiple path as applicable). When adding a **new scenario**, add tests for all applicable frontends. When adding a **new frontend**, add tests for all existing scenarios that apply. Document the scenario and frontend coverage (e.g. in design.md or a testing doc) so the matrix stays up to date. This ensures codegen behavior stays in lockstep for every consumer entry point.

### Code Conventions

- **Durable wording only:** Never use temporal or minimal phrasing in docs or comments. Do not say "first pass," "in this pass," "keywords supported in this pass," "not yet implemented," "not yet feature complete," or "in this release." Document what is implemented and what is ignored; avoid framing by release or phase.
- **Rust conventions and style:** We often review competitor libraries implemented in other languages; our library is written in Rust and **must** adhere to standard Rust conventions, coding style, and best practices. Follow the Rust API Guidelines, idiomatic Rust (e.g. use `Result` for fallible operations, prefer enums over stringly-typed errors, use standard types and traits), and project lint/format rules. Do not mirror non-Rust idioms from competitors when they conflict with Rust conventions.
- Run `make lint test` before completing any changes.
- Use `#[expect]` not `#[allow]` for Clippy overrides.
- Never fail silently; log errors internally (customer-facing message can differ).
- Follow existing patterns: custom Error enum, BTreeMap for ordering, explicit type annotations on all variables.
- **No literal recursion:** Prefer an iterative approach (explicit stack or queue) instead of recursive calls so the crate is not vulnerable to stack overflow on deep inputs. If recursion is needed conceptually, implement it with a loop and a stack.
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
