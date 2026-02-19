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

- **Vendored JSON Schema specs** (`specs/`) — *What:* Offline copy of JSON Schema drafts (draft-00 through 2020-12). *How:* Read HTML/JSON under `specs/json-schema.org/`; run `./research/scripts/list_keywords.sh` from repo root for the canonical keyword list. Use **only** vendored specs—no web. If specs are missing, the maintainer runs `make vendor_specs`. *When:* At the start of every feature (understand how the keyword is defined and how it behaves in each draft); when documenting spec-version quirks in design.md.

- **Research reports** (`research/reports/<lang>/{org}-{repo}.md`) — *What:* Structured reports on competitor libraries (what they support, how they implement features). *How:* Read the report for the feature you're implementing; rank approaches by our values (see design.md); add or update sections when you learn something new. *When:* When implementing a feature (see how others did it before writing code); after implementing (contribute back so the report stays the knowledge source). See the **competitor-json-schema-codegen-analysis** skill for how reports are produced and the report template.

- **Cloned competitor repos** (`research/repos/<lang>/<name>/`) — *What:* Git-ignored vendored clones of competitor codebases. *How:* Read source code when the research report lacks detail. Do not run untrusted code without review. *When:* After reading the research report, when you need code-level or high-granularity detail. Prefer the report first; use clones for depth.

- **design.md** (repo root) — *What:* Design and architecture knowledge bank: high-level architecture, values, design principles, per-keyword implementation notes, spec-by-version behavior, version quirks. *How:* Read the relevant section(s) before implementing a keyword; after implementing, update "Our implementation" and "Spec version quirks" (and related subsections) in design.md. Code remains source of truth for literal behavior; design.md stores rules and reasoning. *When:* At the start of a feature (understand our design or that it's TODO); at the end (capture high-level decisions, edge-case rules, spec-version differences).

- **README.md** (repo root) — *What:* Consumer-facing description: what the library does, what each tool supports, which specs we adhere to, how to run it. *How:* Update when you add/change a user-visible feature or spec support. Keep succinct and maximally insightful for developers evaluating or using the library. *When:* When changing supported features, CLI behavior, or anything users see. Do not put design/architecture detail here—point to design.md.

## Implementation workflow

### Before implementing

**Always read the relevant parts of these resources** before implementing a new feature, so you're aware of everything you need to be aware of:

- **design.md** — Our design and implementation notes for the keyword/feature; what's already implemented vs TODO; spec version quirks.
- **README.md** — What we currently expose to users; which specs we support; how the library is presented.
- **Vendored JSON Schema specs** (`specs/`) — How the keyword is defined and how it behaves in each draft we care about.
- **Competitor research reports** (`research/reports/<lang>/{org}-{repo}.md`) — How other libraries implement this feature; rank by our values (design.md).
- **Locally cloned competitor repos** (`research/repos/<lang>/<name>/`) — When the research report lacks detail, read the source for code-level insight.

Read as much as needed from each; don't skip this step. It ensures we stay spec-aligned, avoid duplicating work, and learn from competitors.

### Implement

Schema model, codegen/validation behavior, tests, examples. Follow Contribution Guidelines below.

### After implementing

1. **Update design.md** — Keep track of which features we've implemented. Add or update the relevant section(s) with our implementation notes, spec version quirks, and any "Currently implemented" summary so design.md stays the single source of truth.
2. **Update the research report** — If you learned something new about a competitor (from a cloned repo or elsewhere) that isn't already covered by the research report, add or update the report so we retain that knowledge for next time.
3. **Update README** — If the change is user-visible (new feature, new keyword support), update the README (see Resources: README.md).

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

### Code Conventions

- Run `make lint test` before completing any changes.
- Use `#[expect]` not `#[allow]` for Clippy overrides.
- Never fail silently; log errors internally (customer-facing message can differ).
- Follow existing patterns: custom Error enum, BTreeMap for ordering, explicit type annotations on all variables.

### Adding New JSON Schema Support

- Add schema model fields only when needed; use `#[serde(default)]` and `Option` so extra keys in the JSON are ignored.
- For unsupported types, decide project policy (ignore vs fail); document in design.md or README as appropriate.

## Repository layout

- **Workspace crates**: `json_schema_rs/` (lib — core logic), `json_schema_to_rust_cli/` (CLI — Schema→Rust frontend). Root `Cargo.toml` defines the workspace only.
- **Vendored JSON Schema specs**: `specs/`
- **Competitor clones**: `research/repos/<lang>/<name>/`
- **Research reports**: `research/reports/<lang>/{org}-{repo}.md`
- **Design and architecture**: `design.md` (repo root)

Key source files are in `json_schema_rs/src/` (e.g. `schema.rs`, `codegen.rs`, `error.rs`). CLI and build commands are documented in the README.
