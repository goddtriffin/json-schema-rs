---
name: contribution-guide
description: Use when contributing to the json-schema-rs crate (adding features, fixing bugs, understanding code layout, researching specs and competitors). For user-facing information (what the crate supports, how to use it), see the [README](README.md).
---

# json-schema-rs (contribution guide)

Supported and unsupported features are documented in the
[README](README.md). This skill focuses on **how to contribute**
to the crate.

## Purpose and Philosophy

The json-schema-rs crate provides **three tools**:

1. **JSON Schema → Rust struct** (codegen): generate Rust types from a JSON Schema.
2. **Rust struct → JSON Schema** (reverse codegen): generate a JSON Schema from Rust types.
3. **JSON Schema validator**: two inputs—JSON Schema definition and JSON instance—output validation result.

**For every feature we develop, update, or fix, implement it for each of these three tools** where the feature applies (some features may apply to only one or two tools).

### Our top values

Our number-one values drive design decisions and how we rank competitors. Best libraries align with these:

- **Spec-adherence**: behavior matches the JSON Schema specification(s) we support.
- **Determinism**: same input always produces same output; stable ordering (e.g. alphabetical).
- **Performance**: efficient algorithms and data structures; avoid unnecessary work.
- **Testability**: core APIs work with in-memory writers (e.g. `Vec<u8>`) and test doubles, not only file I/O.
- **Benchmarks**: we maintain benchmarks so we can measure and guard performance.

### Design Principles

- **Testability-first**: Core APIs should work with writers (e.g. `Vec<u8>` or `Cursor<Vec<u8>>`) so tests avoid file I/O.
- **Deterministic output**: Use stable ordering (e.g. `BTreeMap` for alphabetical struct and field ordering). Same input always produces same output.
- **Schema model**: Only model schema fields we need. Use serde with `#[serde(default)]` and `Option` for optional keys.
- **Errors**: Use a custom error enum with manual `Debug`, `Display`, `Error`, and `From` impls (no thiserror unless the project adopts it).

## Architecture

Architecture will be documented as the three tools are built. We have three separate pipelines: Schema→Rust, Rust→Schema, and the validator. For public API and feature set, see the [README](README.md).

Code layout will be defined as the crate is rebuilt. When adding support for a new keyword or type, consider: schema model, codegen/validation behavior, tests, and examples—without assuming specific file names.

## Contribution Guidelines

### Git

- **Never run `git add`, `git commit`, or `git push`.** The maintainer will
  always handle version control themselves. Make edits and leave staging and
  commits to them.

### Testing

- **Inlined tests**: Most tests have input and expected output inlined in the
  test method (no file loading).
- **File-based test**: When the test suite includes file-based tests (schema +
  expected output), **always update those files** when implementing new features
  so the file-based test exercises the new behavior.
- **Unit tests**: Add `#[cfg(test)]` tests in the relevant module(s) for
  feature-specific logic. **Always write exhaustive unit tests** so that every
  new feature is fully verified. At a minimum, have unit tests that cover
  **success and failure conditions**, and **edge cases** that you are aware of.
  Aim for one unit test per **code path** (e.g. one test for each possible
  outcome or branch), plus **opposite pairings** such as success vs failure,
  enabled vs disabled, bounds present vs absent, or fallback vs non-fallback.
- **Test shape**: Prefer **one assertion per test**. Each test should have an
  `expected` value, an `actual` value, and a single comparison (e.g.
  `assert_eq!(expected, actual)`). Test the **whole scenario**: avoid asserting
  on subsets of `actual` (e.g. no `actual.contains(...)` or checking only one
  field); compare the full value so the test validates the entire behavior.
  **Exceptions**: When the type does not support `PartialEq` (e.g. some error
  types), use a single `assert!(matches!(actual, ...))` with a named `actual`;
  document the expected variant in a comment if helpful.
- **Assertions**: Always use named `expected` and `actual` and a single
  comparison; for string output use full expected strings and
  `assert_eq!(expected, actual)`.
- **Integration tests**: Integration tests use the public API; keep them in the
  integration test module.

### Code Conventions

- Run `make lint test` before completing any changes.
- Use `#[expect]` not `#[allow]` for Clippy overrides.
- Never fail silently; log errors internally (customer-facing message can
  differ).
- Follow existing patterns: custom Error enum, BTreeMap for ordering, explicit
  type annotations on all variables.

### Adding New JSON Schema Support

- Add schema model fields only when needed; use `#[serde(default)]` and `Option`
  so extra keys in the JSON are ignored.
- For unsupported types, decide project policy (ignore vs fail); document in the
  skill or README.

### JSON Schema spec research

For every feature we develop, update, or fix:

- **Research the latest JSON Schema spec** (e.g. draft 2020-12) in `specs/`.
  Understand how the feature is defined and how it behaves.
- **Research all older JSON Schema specs** we care about, using **only vendored
  specs** under `specs/`. Do not rely on the web for spec text. If specs are
  missing, the maintainer runs `make vendor_specs` (or fixes the spec
  download script).

### Competitor research for each feature

For every feature we develop, update, or fix:

- **Find all libraries** (across the researched languages) that implement that
  feature. Research reports live under `research/reports/<lang>/{org}-{repo}.md`.
- **Rank them** from best to worst according to **our top values** (spec-adherence,
  determinism, performance, testability, benchmarks—see Purpose and Philosophy).
- **Focus on the best one or two libraries**; do not copy patterns from badly
  architected or non-spec-aligned implementations.
- **When learning how a library implements the feature:**
  - **Read its research report first.** Reports are the primary knowledge source.
  - If you need high-granularity detail, **then** read the actual code in the
    cloned repo (e.g. `research/repos/<lang>/<name>/`).
  - **Contribute back to the research report:**
    - If a preexisting section is missing details for this feature, add those
      details to that section.
    - If the report has **no section** that covers this feature, add a new
      section so we remember it for future comparison and learning.

See the **competitor-json-schema-codegen-analysis** skill for how reports are
produced and the report template (in that skill’s reference.md).

### Post-Feature Knowledge Capture

At the end of development for a new feature, add **high-level** knowledge back
to this skill (and to research reports when applicable). Capture **abstract
ideas and rules**: edge-case principles, design trade-offs, conventions. **Do
not** capture low-level or highly specialized implementation details here—**code
is the source of truth** for the literal behavior. If we need the exact
behavior later, we read the code; the contribution guide stores the rules and
reasoning that help future contributors.

### Required vs Optional (learned)

- **`required`** is an array of property names at each object schema level. When
  absent, all properties are optional per JSON Schema spec. When `required: []`,
  all properties are optional.
- **Explicit optional (recognized but ignored):** The per-property `optional`
  keyword may be parsed in the schema model and explicitly **ignored** in
  code generation; required vs optional is determined only by the object-level
  `required` array. This is for future-proofing: strict adherence to the JSON
  Schema spec and/or settings may be added later.
- **File-based expected output**: When comparing generated output to an expected
  file, the expected file must match the generator’s output exactly (e.g.
  trailing newlines if the generator emits them).
- **Field ordering**: Use stable ordering (e.g. BTreeMap yields alphabetical
  order by property key).

### Enum support (learned)

- **`enum`** in JSON Schema: array of allowed values. Only string enums
  supported; non-string values fall back to `String`.
- **Variant naming**: Produce PascalCase (first char uppercase, rest lowercase
  per word). Invalid identifiers (e.g. `"123"`) get an `E` prefix.
- **Collision handling**: When multiple JSON values map to same Rust variant
  name (e.g., `"PENDING"`, `"pending"`, `"Pending"` all -> `Pending`), append
  `_0`, `_1`, `_2` to **all** colliding variants.
- **Determinism**: Sort enum values alphabetically before processing. Ensures
  `["markdown", "plain"]` and `["plain", "markdown"]` produce identical output.
- **Deduplication**: Duplicate JSON values (e.g., `["a", "a"]`) deduplicated to
  one variant.
- **Raw string literals in tests**: Use `r#"..."#` not `r"..."` when expected
  output contains `]"` (e.g., `#[serde(rename = "x")]`) to avoid Rust parsing
  the `]"` as end of string.

### Numbers (learned)

- **Mapping**: We use `minimum` and `maximum` (when both present and valid) to
  choose the smallest integer type (i8, u8, i16, u16, i32, u32, i64, u64) or
  float type (f32 vs f64). Fallback: no/min/max or invalid → `i64` for integer,
  `f64` for number. No validation; type selection only. Float selection is
  range-based; f32 may lose precision for some decimals.
- **Arrays**: For `items.type` of `integer` or `number`, use the same
  integer/number type selection logic as for standalone numeric schemas.

### Default support (learned)

- **Schema model**: Preserve JSON `null` in the default value (e.g. Absent vs
  Present(Value)). Serde deserializes `Option<Value>` with null as `None`,
  losing the distinction between absent key and `"default": null`.
- **Two strategies**: `UseTypeDefault` → `#[serde(default)]` when the schema
  value equals the type's Default (false, 0, 0.0, "", [], null for optional).
  `Custom { fn_name, rust_expr }` → generated function +
  `#[serde(default =
  "fn")]` for literal defaults.
- **Optional + null**: For optional fields with `default: null`, use
  `#[serde(default)]` so missing key yields `None`.
- **Emission order**: Enums first, then default functions (they may reference
  enums), then structs (or equivalent for the tool).
- **Custom default function**: Returns `Some(expr)` for optional fields, `expr`
  for required. Emit before the struct (or type) that uses it.
- **Out of scope**: Object defaults, non-empty array defaults.

### Description support (learned)

- **Normalization**: Empty or whitespace-only `description` is treated as
  absent; do not emit blank doc lines.
- **Multi-line**: Emit one doc line per line of text (e.g. `description.trim().lines()`).
  Field doc comments use an appropriate prefix so they align with the field line.
- **Placement**: Object schema `description` → struct doc; enum schema
  `description` → enum doc; property schema `description` → field doc (same
  schema used for the property, including nested object descriptions on the
  field that references that object).

## README.md

The repo’s **root-level README.md** must always be kept up-to-date. It should
communicate: **our tools** (Schema→Rust, Rust→Schema, validator), **features**
(what each tool supports), **which JSON Schema specs we adhere to** (draft
versions), and any other information developers need to use the Rust library.
Aim for **succinct, maximally insightful** content. When adding or changing
features or spec support, update the README so users and contributors always
see an accurate picture.

## Repository layout

- **Workspace crates**: `json_schema_rs/` (lib — core logic),
  `json_schema_to_rust_cli/` (CLI — Schema→Rust frontend). Root `Cargo.toml`
  defines the workspace only.
- **Vendored JSON Schema specs**: `specs/`
- **Competitor clones**: `research/repos/<lang>/<name>/`
- **Research reports**: `research/reports/<lang>/{org}-{repo}.md`

Key source files will be listed in this skill or the README as the crate is
re-established. CLI and build commands will be documented when each tool has a
stable interface.
