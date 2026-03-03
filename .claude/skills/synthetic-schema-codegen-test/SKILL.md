---
name: synthetic-schema-codegen-test
description: Sets up synthetic tests for jsonschemars codegen by creating a dummy Rust crate, generating Rust from JSON Schema definitions, analyzing compilation issues, and documenting findings in todo.txt with minimal viable examples. Use when running codegen analysis on external schemas, setting up regression test harnesses, or adding new bug entries to todo.txt.
---

# Synthetic Schema Codegen Test

Automates setup of a synthetic test harness: dummy crate,
`jsonschemars generate rust`, build analysis, and todo.txt updates. Use this
when analyzing codegen issues from external JSON Schema sources.

## Synthetic Data Only

**For every issue added to todo.txt, create synthetic minimal viable bug data.**
Do not use real schemas from external sources (e.g. user-provided files). Use
placeholder schemas that reproduce the same error type. Example:
`{"type":"object","properties":{"x":{"type":"string"}}}` instead of referencing
actual schema content.

## Workflow

### 1. Create Dummy Lib Crate

Create a lib crate at the desired location (e.g. `schema_models/`):

- **Cargo.toml**: Use the latest Rust edition and rust-version (match the
  json-schema-rs workspace), path deps to json-schema-rs workspace
- **Lints**: Copy strict Rust and Clippy settings from workspace root
  [Cargo.toml](Cargo.toml)
- **lib.rs**: `pub mod generated;`

### 2. Run jsonschemars CLI

```bash
jsonschemars generate rust <path-to-schemas-dir> -o <path>/src/generated
```

Do not post-process the output. Leave all issues as-is for documentation.

### 3. Analyze Build Errors

Run each command in the dummy crate in a **separate run** so issues can be
categorized by tool:

- `cargo build` — compilation errors
- `cargo check` — type-check errors
- `cargo clippy` — lint violations
- `cargo fmt --check` — formatting deviations

Capture and group errors by kind and tool. Cross-reference with
[design.md](design.md).

### 4. Update todo.txt

For each issue category, append an entry with a **minimal viable example
(MVE)**:

```
- [Codegen bug] <Category>: <brief description>.
  MVE:
    Input: <minimal synthetic JSON Schema - not real data>
    Current: <what codegen emits today>
    Expected: <what would fix it>
  Fix: <one-line idea>
```

## MVE Requirements

| Field    | Rule                                                                                      |
| -------- | ----------------------------------------------------------------------------------------- |
| Input    | Minimal synthetic schema that triggers the issue; never reference external schema content |
| Current  | Exact or representative codegen output today                                              |
| Expected | What should be emitted for the issue to be resolved                                       |

## Key Files

- CLI: [json_schema_rs/src/cli/generate.rs](json_schema_rs/src/cli/generate.rs)
- Rust backend:
  [json_schema_rs/src/code_gen/rust_backend.rs](json_schema_rs/src/code_gen/rust_backend.rs)
- Design: [design.md](design.md)
- Todo list: [todo.txt](todo.txt)
