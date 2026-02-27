# json-schema-rs design and architecture

This document is the **design and architecture knowledge bank** for the json-schema-rs crate. It describes how the library is designed, how each JSON Schema feature is (or will be) implemented, and how the JSON Schema specification defines each feature across draft versions.

- **One section per feature/keyword** (or sub-sections for related keywords). Research uses **only** the local specs under `specs/` (draft-00 through 2020-12)—download them with `make vendor_specs`; they are gitignored and not in the repo. No reliance on the web.
- Related features are grouped (e.g. string constraints under Strings, number constraints under Numbers). Each section can have **Spec version quirks** sub-sections for differences between drafts; we implement per the latest supported spec and may expose version-based config where behavior differs.

Implemented keywords: type (object, string, integer, number, array), properties, required, title, enum, items. Other keywords are documented in the sections below. Unknown schema keywords and property types that do not map to generated code are ignored and do not cause an error.

---

## High-level architecture

The crate provides **three tools**:

1. **JSON Schema → Rust struct** (codegen): generate Rust types from a JSON Schema.
2. **Rust struct → JSON Schema** (reverse codegen): generate a JSON Schema from Rust types.
3. **JSON Schema validator**: two inputs—JSON Schema definition and JSON instance—output validation result.

**For every feature we develop, implement it for each of these three tools** where the feature applies.

We have three separate pipelines: Schema→Rust, Rust→Schema, and the validator. Code layout: workspace crates `json_schema_rs/` (library and `jsonschemars` CLI binary) and `json_schema_rs_macro/` (proc-macro `generate_rust_schema!` for compile-time codegen). When adding a new keyword or type, consider: schema model, codegen/validation behavior, tests, and examples.

### JSON Schema validator

The validator takes the same **JsonSchema** type used by codegen and a JSON instance (`serde_json::Value`) and returns `Result<(), Vec<ValidationError>>` (type alias **ValidationResult**). It collects **all** validation errors (no fail-fast) and returns them at the end. Inputs: `&JsonSchema`, `&Value`. Output: `Ok(())` when valid, `Err(errors)` when invalid.

**Supported keywords:** `type` (object, string, integer, number), `required`, `properties` (recursive). Does not resolve `$ref` or `$defs`; additional properties are allowed. The validator reuses the same JsonSchema struct as codegen; one parse, one model. A compiled validator (e.g. tree of validator nodes) can be added for performance; the same schema model would be used.

**Validation errors:** Each `ValidationError` variant includes instance context (actual value, count, length, or "got" type) and the schema constraint where applicable. Display messages are one line per error and actionable (e.g. `/: value "pending" not in enum (allowed: "open", "closed")`; `/: array has 2 item(s), minimum is 3`; `/: value 15 is above maximum 10`). Messages are never truncated; full allowed sets, values, and lengths are shown.

### Official JSON Schema Test Suite

We run the [JSON Schema Test Suite](https://github.com/json-schema-org/JSON-Schema-Test-Suite) via an integration test that validates against all test data in the suite. The suite lives at **`research/json-schema-test-suite/`** (gitignored). Cloning is a **manual prerequisite**: run `make vendor_test_suite` to clone or update it. The test **hard-fails** if the suite directory is missing, with a message to run that command. The test is **ignored** by default and runs only when explicitly executed (e.g. `make test_json_schema_suite` or `cargo test --test json_schema_test_suite -- --ignored`); once we pass 100%, it can be re-enabled in the standard test run. Tests that rely on **remotes** or **`$ref`** resolution (e.g. `refRemote.json`) fail until we support `$ref`. The integration test uses **strict schema parsing** (`JsonSchemaSettings::disallow_unknown_fields(true)`), so any case whose schema contains a keyword we do not support fails at parse time rather than being validated with a truncated schema; this yields a more accurate picture of implementation coverage.

### Our top values

These drive design decisions and how we rank competitors:

- **Spec-adherence**: behavior matches the JSON Schema specification(s) we support.
- **Determinism**: same input always produces same output; stable ordering (e.g. alphabetical).
- **Performance**: efficient algorithms and data structures; avoid unnecessary work.
- **Testability**: core APIs work with in-memory writers (e.g. `Vec<u8>`) and test doubles, not only file I/O.
- **Benchmarks**: we maintain benchmarks so we can measure and guard performance.

### Design principles

- **Testability-first**: Core APIs should work with writers (e.g. `Vec<u8>` or `Cursor<Vec<u8>>`) so tests avoid file I/O.
- **Deterministic output**: Use stable ordering (e.g. `BTreeMap` for alphabetical struct and field ordering). Same input always produces same output.
- **Schema model**: Only model schema fields we need. Use serde with `#[serde(default)]` and `Option` for optional keys.
- **Errors**: Use a custom error enum with manual `Debug`, `Display`, `Error`, and `From` impls (no thiserror unless the project adopts it).
- **No literal recursion**: Use an explicit stack (or queue) and iterative loops instead of recursive calls so depth is limited by heap, not call stack. Avoids stack overflow on deeply nested schemas or instances.

### Schema model: struct vs enum

We represent the in-memory schema as a **struct** (`JsonSchema`) with optional fields, not as a Rust enum of schema subtypes (e.g. `ObjectSchema | StringSchema | ...`). Rationale: the JSON Schema spec defines a schema as a **single JSON object with optional keys**; a struct mirrors that shape and keeps deserialization simple. By contrast, `serde_json::Value` is an enum because a JSON *value* is exactly one of several mutually exclusive kinds (Null, Bool, Number, String, Array, Object)—a different domain. Competitor Rust libraries (e.g. schemafy, typify/schemars) use either a typed struct or an untyped Value wrapper; none use an enum of schema subtypes. Using an enum would complicate deserialization and duplicate shared metadata (e.g. title) across variants without clear benefit for our supported keyword subset.

### Settings and spec version

**JsonSchemaSettings** control how JSON Schema definitions are ingested (parsed). Use `JsonSchemaSettings::default()` for default settings (equivalent to `JsonSchemaSettings::builder().build()`), or `JsonSchemaSettings::builder()` to construct custom settings; options include `disallow_unknown_fields` (when `true`, reject schema objects that contain keys other than `type`, `properties`, `required`, `title`). **SpecVersion** is an enum with one variant per vendored spec (Draft00 through Draft202012); `default_schema_settings()` returns a `JsonSchemaSettings` tuned for that spec. Schema is parsed via **TryFrom** for default settings (e.g. `JsonSchema::try_from(json_str)`, `path.try_into()`), or constructor-style **`JsonSchema::new_from_str`**, **`JsonSchema::new_from_slice`**, **`JsonSchema::new_from_serde_value`**, **`JsonSchema::new_from_reader`**, **`JsonSchema::new_from_path`** (all take `&JsonSchemaSettings`) for custom settings. I/O errors from reader/path are reported as **JsonSchemaParseError::Io**. The CLI and macro build settings from flags or builders and pass them through. **CodeGenSettings** are language-agnostic codegen options (e.g. `model_name_source`). Use `CodeGenSettings::builder()`. The **CodeGenBackend** trait (in `code_gen/mod.rs`) takes `&CodeGenSettings` in `generate`. The Rust backend and `generate_rust` live in `code_gen/rust_backend.rs`; settings live in `code_gen/settings.rs`. CLI prefixes: `jss-` (JSON Schema Settings), `cgs-` (codegen settings); future Rust-specific: `cgs-rs-`.

### Codegen backends

Codegen is built around a **swappable backend** trait in **`code_gen/mod.rs`**: input is a slice of `JsonSchema`, **CodeGenSettings**, and output is **GenerateRustOutput** `{ shared: Option<Vec<u8>>, per_schema: Vec<Vec<u8>> }`. The trait `CodeGenBackend` has a single method, `generate(&self, schemas: &[JsonSchema], settings: &CodeGenSettings) -> Result<GenerateRustOutput, Error>`. The **CLI** builds `CodeGenSettings` and `JsonSchemaSettings` from flags and calls the corresponding backend. The only implementation today is **Rust** (`RustBackend` in **`code_gen/rust_backend.rs`**), which emits serde-compatible Rust structs; when dedupe is enabled (default), structurally identical object schemas within and across schemas are emitted once in a shared buffer. The public API is `generate_rust(schemas: &[JsonSchema], settings: &CodeGenSettings) -> Result<GenerateRustOutput, Error>`; callers use `output.per_schema` (one buffer per schema) and optionally `output.shared`. **Adding another language:** implement `CodeGenBackend` for a new type (e.g. `PythonBackend`) in a new module (e.g. `code_gen/python_backend.rs`), add a match arm in the CLI’s `run_generate` for the language name (case-insensitive), and update the "supported" text in the unsupported-language error message (e.g. "supported: rust, python").

**Codegen entry points:** (1) **CLI** — one or more INPUTs (file paths, directory paths recursively searched for `.json`, or `-` for stdin); required `-o` output directory. CLI flags: `--jss-disallow-unknown-fields`, `--cgs-model-name-source title-first|property-key`, `--cgs-dedupe-mode disabled|functional|full`. Schema file ingestion uses `JsonSchema::new_from_slice` with `JsonSchemaSettings` built from flags; each failure is logged to stderr and the command exits with failure after all files have been attempted (no output is written if any ingestion fails). Generated output path components are **sanitized** so that file and directory names are valid Rust identifiers. When dedupe produces shared structs, the CLI writes them to **`shared.rs`** in the output directory and adds `pub mod shared;` to the root `mod.rs`; per-schema files reference shared types via `crate::shared::Typename`. (2) **Library** — `generate_rust(schemas, &CodeGenSettings::builder().build())` returns **GenerateRustOutput** (`shared`, `per_schema`). Use `output.per_schema` for one buffer per schema; use `output.shared` when present for the shared definitions. Parse schemas with `JsonSchema::try_from(json)` (or `.try_into()`) for default settings, or `JsonSchema::new_from_str`, `new_from_slice`, `new_from_serde_value`, `new_from_reader`, `new_from_path` with `&JsonSchemaSettings::default()` or custom settings. (3) **Macro** — the `json-schema-rs-macro` crate provides `json_schema_to_rust!(...)`, which runs at compile time and **inlines** the generated Rust at the call site. Use `json_schema_rs_macro::json_schema_to_rust`. The macro builds `JsonSchemaSettings` and `CodeGenSettings` via their builders (no args) and calls `RustBackend::generate(&schemas, &code_gen_settings)`; when `output.shared` is present it emits a `shared` submodule; module name from file stem for paths, or `schema_0`, `schema_1`, … for inline. Macro-related tests and fixtures live in the macro crate; the main crate has no macro-specific tests. Consumers add both `json-schema-rs` and `json-schema-rs-macro` and use `json_schema_rs_macro::json_schema_to_rust`. A re-export from the main crate would require the main crate to depend on the macro crate, which would create a cyclic workspace dependency (the macro crate depends on the main crate for codegen).

**Model deduplication.** When **DedupeMode** is not **Disabled** (default is **Full**), the Rust backend deduplicates structurally identical object schemas **within** a single schema and **across** multiple schemas. One Rust struct is generated per equivalence class; shared structs are emitted in the **shared** buffer (e.g. `shared.rs`), and per-schema buffers contain only structs used in that schema plus `pub use crate::shared::...` for shared types they reference. Equality is **deep** (nested objects must match under the same rules). **Functional** mode compares only pivotal/functional data (type, properties, required, title, constraints); **Full** mode also compares non-functional fields (e.g. `description`). Dedupe uses a **DedupeKey** (built from `JsonSchema` and mode) with **Ord** + **Eq** in a **BTreeMap** for deterministic, idempotent output. Canonical struct name is the first occurrence's name (by schema index and traversal order). When dedupe is **Disabled**, `shared` is always `None` and `per_schema` is the same as the previous one-buffer-per-schema behavior.

**Codegen tests: compile and deserialize.** We verify that generated Rust compiles and that serde deserialization works by writing generated code into temporary Cargo crates (edition 2024, lib + binary), running `cargo build`, then running a binary that deserializes a fixed JSON string into the generated type(s). Integration tests in **`json_schema_rs/tests/integration.rs`** use a **single workspace test** (`generated_rust_build_and_deserialize_all_scenarios`): one temp workspace with one package per scenario (unique crate names), `cargo build --workspace` once with **shared `CARGO_TARGET_DIR`** (e.g. `CARGO_TARGET_DIR/integration_codegen`) so path dependencies (json-schema-rs, json-schema-rs-macro) are built once and reused, then `cargo run -p <name>` for each scenario. The workspace has **nine** members: **kitchen_sink** (one schema combining many features—required/optional primitives, arrays, uniqueItems, min/max items, nested object, description, string min/max length, hyphenated property, enum—with one `main` asserting the same behavior as the former single-feature scenarios), **round_trip** (one crate with six modules, each running the same round-trip assertion as the former round_trip_* scenarios), **enum_variants** (one crate with three modules for enum_collision, enum_dedupe, enum_duplicate_values), plus **nested_modules**, **multi_schema**, **hyphenated_paths**, **dedupe_two_identical**, **model_name_source_property_key**, and **deep_nesting**. Each scenario has one assertion; failure messages include the scenario name. Temp crates that compile generated code with the ToJsonSchema derive add **json-schema-rs** and **json-schema-rs-macro** as path dependencies so the derive and trait resolve.

### Rust struct → JSON Schema (reverse codegen)

**Entry points:** (1) **Generated structs** — every struct emitted by forward codegen (library, CLI, macro, build+deserialize) has `#[derive(..., json_schema_rs_macro::ToJsonSchema)]` and optional container/field attributes (e.g. `#[to_json_schema(title = "Root")]`). No stored schema constant; the derive builds the schema from the type and attributes. (2) **Hand-written structs** — use `#[derive(ToJsonSchema)]` from `json_schema_rs_macro` with optional `#[to_json_schema(...)]` and `#[json_schema(...)]` attributes. Consumers need **json-schema-rs** (trait, `JsonSchema` type) and **json-schema-rs-macro** (derive) in scope; generated code uses the full path `json_schema_rs_macro::ToJsonSchema` in the derive list.

**Trait and serialization:** The **ToJsonSchema** trait (in **`reverse_code_gen`**) has `fn json_schema() -> JsonSchema`. **JsonSchema** implements **Serialize** and **TryFrom<&JsonSchema> for String** / **TryFrom<&JsonSchema> for Vec<u8>** (and consuming forms); use `String::try_from(&schema)` or `.try_into()` to get JSON. Error type is **JsonSchemaParseError** (wraps `serde_json::Error`). Round-trip: parse schema → generate Rust → compile crate with json-schema-rs + macro → for each generated type call `TypeName::json_schema()` → TryFrom to String/Vec<u8> → parse back → assert equals original (or derived) schema.

**Container/field attributes:** Container: **`#[to_json_schema(title = "...")]`** (when the schema had a title). Field-level **`#[to_json_schema(minimum = N, maximum = N)]`** is supported for emitting JSON Schema bounds on a property; N can be integer or float literals (stored as f64). When present, the attribute value overrides the type-derived minimum/maximum (e.g. an `i64` field with `#[to_json_schema(minimum = 0, maximum = 255)]` emits a schema with those bounds). Field-level **`#[to_json_schema(min_items = N, max_items = M)]`** is supported for array/set properties (Vec, HashSet, Option<Vec>, Option<HashSet>); when present, the attribute overlays the type-derived schema so the emitted JSON Schema includes minItems/maxItems. Field-level **`#[to_json_schema(min_length = N, max_length = M)]`** is supported for string properties (String, Option<String>); when present, the attribute overlays the type-derived schema so the emitted JSON Schema includes minLength/maxLength. Other field attributes (e.g. `#[json_schema(...)]`) are parsed for future use; only supported schema keywords (type, properties, required, title, minimum, maximum, min_items, max_items, min_length, max_length) are emitted today. Constraint attributes (pattern, etc.) are reserved until the schema model and validator support them. Attribute names follow a Serde-style pattern (container vs field). **No literal recursion** in `reverse_code_gen`: schema construction and serialization use iteration + stack where depth can be large (see design principle above).

### Codegen tests: scenario × frontend

We test each **codegen scenario** (a named situation: e.g. single required string, nested object, hyphenated key, dedupe) across every **applicable** codegen frontend so behavior stays in lockstep for every consumer entry point. See the contribution guide (Testing → Codegen scenario × frontend coverage) for the requirement. When adding a new scenario, add tests for all applicable frontends; when adding a new frontend, add tests for all existing scenarios that apply. Keep the matrix below up to date.

**Frontends:**

| Frontend | What it tests | Where tests live |
|----------|----------------|------------------|
| Golden string | Library API: `JsonSchema::new_from_str` + `generate_rust`; assert full generated string equals expected. | `json_schema_rs/src/code_gen/rust_backend.rs` (unit), `json_schema_rs/tests/integration.rs` (e.g. `integration_parse_and_generate`) |
| CLI | Run `jsonschemars generate rust -o DIR <inputs>`; assert exit code and output file contents. | `json_schema_rs/tests/integration.rs` (`cli_generate_rust_*`) |
| Generated Rust build + deserialize | One workspace test: generate code per scenario, write to workspace members, `cargo build --workspace` (shared `CARGO_TARGET_DIR`), `cargo run -p <name>` per scenario with `serde_json::from_str` into generated types. | `json_schema_rs/tests/integration.rs` (`generated_rust_build_and_deserialize_all_scenarios`) |
| Macro | `json_schema_to_rust!(...)` (inline and/or path); expanded code compiles and types/values behave as expected. | `json_schema_rs_macro/tests/` (`macro_single_inline`, `macro_single_path`, `macro_multiple_inline`, `macro_multiple_path`) |

**Scenario × frontend matrix** (Y = covered, N = not covered, — = not applicable):

| Scenario | Golden (unit) | Golden (integration) | CLI | Build+deserialize | Macro |
|----------|---------------|------------------------|-----|-------------------|-------|
| type: object (root and nested) | Y | Y | Y | Y | Y |
| Single schema, required string | Y | Y | Y | Y | Y |
| Single schema, optional string | Y | Y | Y | Y | Y (single_path) |
| Nested object (single schema) | Y | Y | Y | Y | Y |
| Two flat schemas | Y | — | Y | Y | Y |
| Nested dir layout (root + nested/child) | — | — | Y | Y | — |
| Hyphenated property key (foo-bar) | Y | Y | Y | Y | Y |
| Hyphenated paths (schema-1, sub-dir) | — | — | Y | Y | — |
| Single required integer | Y | Y | Y | Y | Y |
| Single optional integer | Y | Y | Y | Y | Y |
| Single required number (float) | Y | Y | Y | Y | Y |
| Single optional number (float) | Y | Y | Y | Y | Y |
| Integer with min+max (narrow type, e.g. u8) | Y | Y | Y | Y | Y |
| Number with min+max (f32 when in range) | Y | Y | Y | Y | Y |
| Dedupe (two identical schemas) | Y | — | Y | Y | — |
| Model name source (property-key) | Y | — | Y | Y | — |
| Root not object / empty object error | Y | Y | — | — | — |
| Batch error index | Y | — | — | — | — |
| CLI ingestion errors | — | — | Y | — | — |
| Deep nesting (no stack overflow) | Y | — | Y | Y | — |
| Required enum property | Y | Y | Y | Y | Y |
| Optional enum property | Y | Y | Y | Y | Y |
| Enum with variant collision (e.g. "a" and "A") | Y | — | Y | Y | — |
| Enum dedupe (same enum in two properties) | Y | — | Y | Y | — |
| Enum with duplicate values in schema | Y | Y | Y | Y | — |
| Non-string enum → String fallback | Y | — | — | — | — |
| Const string property (single-value enum) | Y | Y | Y | Y | Y |
| Reverse codegen (every struct ToJsonSchema + attributes) | Y | Y | Y | Y | Y |
| Round-trip (generate → Type::json_schema() → TryFrom → parse → equals) | — | Y | — | Y | Y |
| description (struct, field, enum doc) | Y | Y | Y | Y | Y |
| $comment (round-trip; reverse via comment attribute) | Y | Y | Y | Y | Y |
| $id (round-trip; id attribute emitted when present; reverse via id attribute) | Y | — | — | — | — |
| Required array property (e.g. Vec\<String\>) | Y | Y | Y | Y | Y |
| Optional array property | Y | Y | Y | Y | Y |
| Array with uniqueItems true (e.g. HashSet\<String\>) | Y | Y | Y | Y | Y |
| Array with minItems/maxItems (schema + emitted attribute) | Y | Y | Y | Y | Y |
| String with minLength/maxLength (schema + emitted attribute) | Y | Y | Y | Y | Y |
| allOf (merged object) | Y | — | — | Y | — |
| Array of objects (nested struct) | Y | Y | Y | Y | Y |
| Array of arrays (e.g. Vec\<Vec\<String\>\>) | Y | — | Y | Y | Y |

### Rust codegen: name sanitization

All functions that produce valid Rust identifiers (struct names, field names, module names, path components) live in **`json_schema_rs/src/sanitizers.rs`** as a single source of truth.

**Functions and roles:**

- **`to_pascal_case(name)`** — Converts to PascalCase for type names. Splits on `_`, `-`, space; capitalizes each word. Empty → `"Unnamed"`; leading digit → `N{out}`. Non-ASCII → `_`.
- **`sanitize_struct_name(s)`** — Type/struct/enum names. Uses `to_pascal_case`; then if result is Rust keyword `Self`, appends `_` → `Self_`. Leading digit already prefixed in `to_pascal_case`.
- **`sanitize_field_name(key)`** — Field identifiers (snake_case). Replaces `-` with `_`; invalid chars → `_`. Empty → `"empty"`; leading digit → `field_{s}`; single `_` → `"empty"`. Rust strict/reserved keywords (e.g. `type`, `self`) get trailing `_` (e.g. `type_`). Codegen emits `#[serde(rename = "...")]` when field name differs from JSON key. Non-ASCII → `_`.
- **`sanitize_module_name(s)`** — Module names. Replaces `-`, `.`, space with `_`; keeps `[a-zA-Z0-9_]`. Empty → `"schema"`; leading digit → `schema_{s}`; reserved `crate`/`self`/`super` → `{s}_mod`. Non-ASCII → `_`.
- **`sanitize_path_component(component)`** — File stem or dir name for output paths. Replaces `-` and non-`[a-zA-Z0-9_]` with `_`. Empty → `"schema"`; leading digit → `_{s}`. Non-ASCII → `_`.
- **`sanitize_output_relative(relative)`**, **`module_name_from_path(path)`** — Build on the above.

**Rules summary:**

| Rule | Type/struct | Field | Module | Path component |
|------|-------------|-------|--------|----------------|
| Empty | `Unnamed` | `empty` | `schema` | `schema` |
| Leading digit | `N{out}` | `field_{s}` | `schema_{s}` | `_{s}` |
| Invalid/non-ASCII | `_` (in PascalCase input) | `_` | `_` filtered | `_` |
| Rust keyword | `Self` → `Self_` | keyword → `{kw}_` | `crate`/`self`/`super` → `{s}_mod` | — |

**Stability guarantee:** Sanitizer output is deterministic and intended to be stable across versions. Any change will be documented and rare (e.g. security or spec compliance). Unit tests lock golden input→output pairs (e.g. `"type"` → field `type_`, struct `Type`; `"self"` → struct `Self_`).

**Competitor comparison:** Typify uses heck + custom sanitize; enum variant uniqueness via replacing non-identifier chars with `"X"`; rust-collisions fixture for keywords. schemafy uses Inflector for Pascal/snake; Rust keywords and invalid identifiers escaped with trailing underscore and serde rename. We use a single module, explicit keyword set (strict + reserved from the Rust Reference), and trailing `_` for type (`Self`) and field keywords so generated code is always valid without raw identifiers.

**Duplicate struct names:** When two schemas (or title vs property key) produce the same sanitized struct name, codegen currently keeps the first occurrence and skips the second (“first wins”). Future work may add disambiguation (e.g. numeric suffix) or explicit failure.

---

## 1. Core / identification

### $schema

**Our implementation:** We parse, store, and serialize `$schema` (round-trip). It is stored as `schema: Option<String>` on [`JsonSchema`] (serde `rename = "$schema"`). The validator accepts and preserves it; it does not change validation outcome. Codegen does not use it to alter generated Rust. **Draft inference:** When [`JsonSchemaSettings::spec_version`] is `None` (default), the effective spec version is inferred from the root schema's `$schema` via [`SpecVersion::from_schema_uri`]; if `$schema` is absent or the URI is unrecognized, we use **Draft 2020-12**. Use `resolved_spec_version(schema, settings)` to obtain the effective version. **Reverse codegen:** The `ToJsonSchema` derive emits a default `$schema` of `https://json-schema.org/draft/2020-12/schema` on the root schema so emitted documents are self-describing. **SpecVersion API:** [`SpecVersion::schema_uri()`] returns the canonical meta-schema URI for each variant; [`SpecVersion::from_schema_uri(s)`] parses a `$schema` URI string and returns the corresponding variant (or `None` for unknown/empty). Legacy draft-04 URI `http://json-schema.org/schema#` is accepted and maps to Draft04.

**Spec version quirks:** Draft 4 deprecated `http://json-schema.org/schema#` ("latest version"); specific version URIs are required for clarity. All drafts define `$schema` as an optional string (URI). It declares the dialect/meta-schema; it is not a validation keyword on instance data. Older drafts (00–02) used hyper-schema URIs in meta-schema files; draft-03 onward use `schema#` or (2019-09, 2020-12) `https://json-schema.org/draft/YYYY-MM/schema`.

### $id

**Our implementation:** We parse, store, and serialize `$id` (round-trip). It is stored as `id: Option<String>` on [`JsonSchema`] (serde `rename = "$id"`). We support **only `$id`**; we do **not** support draft-04 `id` (no parsing or emission of the un-prefixed keyword). The validator accepts and preserves it; it does not change validation outcome. Codegen does not use it for struct naming. **Dedupe:** **Full** dedupe mode includes `id` in the dedupe key (two otherwise-identical schemas with different `$id` produce two structs). **Functional** dedupe mode does **not** include `id` in the key (same shape with different `$id` yields one shared struct). Reverse codegen: `#[to_json_schema(id = "...")]` on a struct sets the emitted schema's `$id`. When we implement `$ref` resolution in a future change, `$id` will be used as the base URI; no `$ref` behavior in this implementation.

**Spec version quirks:** In the JSON Schema specification, draft-04 and earlier use the keyword `id` (no dollar sign); draft 6 and later use `$id`. We support only `$id`; draft-04 `id` is not accepted or emitted. Documents written to draft-04 that need an identifier must use a tool or post-process to emit `id` if required.

### $anchor

TODO.

**Spec version quirks:** (placeholder or blank)

### $dynamicAnchor

TODO.

**Spec version quirks:** (placeholder or blank)

### $dynamicRef

TODO.

**Spec version quirks:** (placeholder or blank)

### $vocabulary

TODO.

**Spec version quirks:** (placeholder or blank)

### $comment

**Our implementation:** We parse, store, and serialize `$comment` (round-trip). The validator ignores it (no validation effect). Codegen does not emit it as user-facing doc (reserved for `description`); we do not emit `//` line comments in generated Rust. Dedupe: **Functional** mode excludes `comment` from the key (same shape with different `$comment` yields one struct); **Full** mode includes it (different comments yield separate structs). Reverse codegen: `#[to_json_schema(comment = "...")]` on a struct or enum sets the emitted schema’s `$comment`.

**Spec version quirks:** `$comment` is defined in draft-07 and later (2019-09, 2020-12); draft-06 and earlier do not define it. We accept the key when present in older drafts (lenient) but only draft-07+ define the keyword.

**Competitor approaches:** Rust: BelfordZ stores `comment` in the schema model; Stranger6667/typify do not. Other languages: many (json-everything, Corvus, networknt, santhosh-tekuri, pwall567, etc.) parse and store with no validation/codegen emission; jsonschema2pojo emits as Javadoc (spec says “should not be used to communicate to users”); Blaze emits only in Exhaustive mode. We align with parse+store+round-trip and no user-facing emission. See research reports under `research/reports/`.

---

## 2. Reference / reuse

### $ref

TODO.

**Spec version quirks:** (placeholder or blank)

### $defs

TODO.

**Spec version quirks:** (placeholder or blank)

---

## 3. Composition

### allOf

The JSON Schema `allOf` keyword is an array of schemas; an instance validates if it validates against **every** element. (Draft-03 and later; draft-00/01/02 do not define allOf in the core meta-schema we vendor.)

**Our implementation:**

- **Ingestion:** We store `allOf` as-is. The in-memory `JsonSchema` has `all_of: Option<Vec<JsonSchema>>`. No merging at parse time.
- **Validator:** When `all_of` is present and non-empty, we validate the instance against **each** subschema (push each `(subschema, instance, path)` onto the validation stack); we collect all errors (no fail-fast). No merge.
- **Codegen:** When a schema node has `all_of`, we **merge on-the-fly** (in codegen only) into a single effective schema for that node, then run existing struct collection and emission on that merged result. We emit a **single** Rust model per node. Merge rules: properties union (same key: recursive merge for nested object/array items; first non-None for leaf/constraint); required union (dedupe, stable order); type_ `Some("object")` when all subschemas are object-like; title/description first non-empty; other keywords first non-None. If merge fails (e.g. conflicting property types, non-object subschema), we return a specific [`CodeGenError`](json_schema_rs/src/code_gen/error.rs) variant (see Codegen errors for merge failures below).
- **Reverse codegen:** **Not supported.** We do not emit `allOf` from Rust types (no macro annotation). Round-trip is intentionally lossy unless a future design adds support (e.g. attributes or a different representation).

**Round-trip and reverse codegen:** Schema round-trip (parse → serialize) and reverse codegen do **not** preserve `allOf`; the design is intentionally lossy. We do not add round-trip tests that assert allOf is preserved.

**Codegen errors for merge failures:** When on-the-fly merging fails, we return distinct `CodeGenError` variants: `AllOfMergeEmpty`, `AllOfMergeNonObjectSubschema`, `AllOfMergeConflictingPropertyType`, `AllOfMergeConflictingNumericBounds`, `AllOfMergeConflictingEnum`, `AllOfMergeUnsupportedSubschema`. We do not silently fall back. Which conflicts are fatal vs resolved (e.g. first-wins for title/description) is documented in the merge implementation and in this section.

**Spec version quirks:** allOf appears in draft-03 and later; behavior is consistent across drafts we support (array of schemas; instance must validate against every element). Draft-00, 01, 02 do not define allOf in the core meta-schema.

### anyOf

TODO.

**Spec version quirks:** (placeholder or blank)

### oneOf

TODO.

**Spec version quirks:** (placeholder or blank)

### not

TODO.

**Spec version quirks:** (placeholder or blank)

### if / then / else

TODO.

**Spec version quirks:** (placeholder or blank)

---

## 4. Type and value constraints

### type

The JSON Schema `type` keyword constrains the instance to one or more primitive types. In the spec it can be a **string** (single type) or an **array of type strings** (instance valid if it matches any listed type). Primitive type names are consistent across drafts: `array`, `boolean`, `integer`, `null`, `number`, `object`, `string`.

**Our implementation:** We accept both a single type string and an array of type strings at parse time; we store and use only the **first** type. `object`, `string`, `integer`, and `number` drive codegen (integer emits `i64` / `Option<i64>`; number emits `f64` / `Option<f64>`; no numeric constraints yet); other types are ignored for codegen but can be used for validation. We do not support draft-03 style array elements that are schema objects (we only interpret type-name strings). See schema model and parsing/constructors in `json_schema_rs/src/json_schema/json_schema.rs`, Rust codegen in `json_schema_rs/src/code_gen/rust_backend.rs`, and validator in `json_schema_rs/src/validator/mod.rs`. For type `"string"` specifically, see **7. Strings** below.

**Limitation (type array):** When the schema has `"type": ["object", "null"]` (or any array of types), we treat it as the first type only. Validation requires the instance to match that single type; we do not implement "instance valid if it matches any type in the array" (e.g. `null` would fail for `["object", "null"]`). This can be implemented in the future if needed.

**Spec version quirks:**

- **Draft-00, 01, 02:** The `type` keyword may be a string or an array. In the meta-schema, array items may be a type string or a schema object (`$ref`). We accept type-name strings only; we do not support schema objects in the type array.
- **Draft-03:** The `type` keyword may be a string or an array. When it is an array, elements may be **type strings or schema objects** (nested schemas). We do not support schema objects in the type array; we only accept type-name strings.
- **Draft-04 and later:** The type array is restricted to **type strings only** (no schema objects). Array must have at least one element; elements must be unique. Same primitive type names in all drafts (simpleTypes: array, boolean, integer, null, number, object, string). Draft-05 has no schema.json in our vendor (PDFs only); type behavior matches draft-04/06 for our purposes.

### const

The JSON Schema `const` keyword (draft-06 and later) requires the instance to be exactly equal to the schema's const value (JSON value equality). It is equivalent to `"enum": [value]` with one element. We support it in the validator, codegen, and reverse codegen.

**Our implementation:**

- **Validator:** When `const_value` is present, the instance must equal that value; otherwise we push `ValidationError::NotConst` with instance path, expected (const) value, and actual (instance) value. We check const before enum; when both are present, satisfying const is sufficient.
- **Codegen:** Only string const is supported for codegen. When a property (or array items) schema has `const_value` that is a string, we treat it as a single-value string enum and reuse the existing enum emission path (one Rust enum with one variant). Non-string const (number, boolean, null, object, array) falls back to the existing type for that node (e.g. String or the type implied by other keywords) so that generated code always compiles.
- **Reverse codegen:** When a Rust unit enum has exactly one variant, we emit `const_value: Some(value)` and `enum_values: None` (i.e. `"const": <value>` in JSON). The value comes from the variant name or `#[serde(rename)]`. Multi-variant unit enums continue to emit `enum_values`.
- **allOf merge:** When merging string subschemas, if both have `const_value`, they must be equal or we return `CodeGenError::AllOfMergeConflictingConst`.

**Spec version quirks:**

- **Draft-04 and earlier:** The `const` keyword does not exist. Strict parsing allows `const` in the schema (we do not reject it for older drafts); validation and codegen apply when the key is present.
- **Draft-06, 07, 2019-09, 2020-12:** `const` is defined: instance must be equal to the const value. Behavior is identical across these drafts.

### enum

JSON Schema `enum` is an array of allowed values. The instance validates if it is equal to one of the elements. We support it in codegen (as a property type), the validator, and reverse codegen.

**Our implementation:**

- **Codegen:** Only string enums are supported. When a property schema has `enum` with at least one value and all values are strings, we emit a Rust enum type (name from property key or title, same as struct naming). Non-string values or mixed types cause fallback to `String`. Variant naming: each value is mapped to PascalCase via `to_pascal_case`; if the result is not a valid Rust type identifier (e.g. starts with digit, keyword `Self`), we prefix with `E`. When multiple JSON values map to the same variant name (e.g. `"a"` and `"A"` both → `A`), we append `_0`, `_1`, `_2` to **all** variants in that collision set. Values are sorted alphabetically for determinism. Duplicate JSON values in the schema produce a single Rust variant. Enum types are deduplicated across properties and schemas: the same set of string values yields one Rust enum (canonical name from first occurrence). Enums are emitted before structs so struct fields can reference them. Root schema must still be `type: "object"` with `properties`; enum is supported only as a property type.
- **Validator:** If `enum_values` is present and non-empty, the instance must be equal to one of the values (using JSON value equality). When both `type` and `enum` are present, we validate type first then enum membership. Error: `NotInEnum` with instance path.
- **Reverse codegen:** `JsonSchema` serializes `enum_values` when present. The `ToJsonSchema` derive supports Rust unit enums: variant name (or `#[serde(rename)]`) becomes the allowed values; we emit `type_: Some("string")` and `enum_values: Some(vec![...])`.
- In tests, use `r#"..."#` when expected output contains `]"`.

**Spec version quirks:**

- **draft-00, 01, 02:** `enum` is an array; instance must be one of the values. Meta-schema: type "array", optional. No minItems or uniqueness requirement in meta-schema.
- **draft-03, 04:** Validation text says the array MUST have at least one element and elements MUST be unique. draft-04 meta-schema: `minItems: 1`.
- **draft-06, 07, 2019-09, 2020-12:** Validation text says the array SHOULD have at least one element and elements SHOULD be unique. Meta-schema: `items: true` (any JSON value). We accept empty enum arrays (no validation constraint from enum) and allow duplicates in the schema (we dedupe in codegen).

---

## 5. Objects

### type: "object"

When a schema has `"type": "object"`, the instance must be a JSON object (a mapping of string keys to values). This meaning is **identical across all JSON Schema drafts** (draft-00 through 2020-12). We use it in codegen (root and nested schemas must have `type: "object"` and `properties` to generate a struct) and in the validator (when `type_ == Some("object")`, we require `instance.as_object().is_some()` and then validate `required` and `properties`). If the schema has `"type": ["object", "null"]`, we store only the first type (`object`) and validate accordingly; see the type-array limitation under **type** above.

**Spec version quirks:** None for the meaning of "object"; the only draft differences are in the format of the `type` keyword (string vs array, and draft-03 array may contain schema objects), documented under **type**.
### properties

We use `properties` to build structs: each property becomes a struct field. Property keys are sanitized for Rust (e.g. `-` → `_`). When the Rust field name differs from the JSON key, we emit `#[serde(rename = "...")]`. Object schemas are traversed recursively; each object with `properties` yields a Rust struct. See `json_schema/json_schema.rs` and `code_gen/rust_backend.rs`.

**Spec version quirks:** (placeholder or blank)

### required

The `required` array lists property names that are required at that object level. When absent, all properties are optional per JSON Schema spec. When `required: []`, all properties are optional. We emit required properties as `T` and optional as `Option<T>`. The per-property `optional` keyword may be parsed and explicitly **ignored** in codegen; required vs optional is determined only by the object-level `required` array (future-proofing). Field ordering: stable (e.g. BTreeMap yields alphabetical order by property key). File-based expected output must match generator output exactly (e.g. trailing newlines).

**Spec version quirks:** (placeholder or blank)

### additionalProperties

TODO.

**Spec version quirks:** (placeholder or blank)

### patternProperties

TODO.

**Spec version quirks:** (placeholder or blank)

### propertyNames

TODO.

**Spec version quirks:** (placeholder or blank)

### minProperties / maxProperties

TODO.

**Spec version quirks:** (placeholder or blank)

### dependentRequired

TODO.

**Spec version quirks:** (placeholder or blank)

### dependentSchemas

TODO.

**Spec version quirks:** (placeholder or blank)

### unevaluatedProperties

TODO.

**Spec version quirks:** (placeholder or blank)

---

## 6. Arrays

### type: "array"

When a schema has `"type": "array"`, the instance must be a JSON array. This meaning is **identical across all JSON Schema drafts** (draft-00 through 2020-12). We use it in codegen (as a property type: array with `items` yields `Vec<T>` or `Option<Vec<T>>`), in the validator (when `type_ == Some("array")`, we require `instance.is_array()` and optionally validate each element against `items`), and in reverse codegen (`Vec<T>` emits `type: "array"` and `items: T::json_schema()`). Array is supported only as a **property type**; root schema must still be `type: "object"` with `properties`.

**Spec version quirks:** None for the meaning of type `"array"`. The only draft differences are in the **form** of the `type` keyword (string vs array), documented under **4. Type and value constraints → type**.

### items

The `items` keyword defines the schema that all array elements must validate against when it is a **single schema** (object). We support only the single-schema form; we do not support `items` as an array of schemas (tuple typing) or draft 2020-12 `prefixItems` in this implementation.

**Our implementation:** We parse and store `items` as `Option<Box<JsonSchema>>`. When present with `type: "array"`, codegen emits `Vec<T>` (or `Option<Vec<T>>`) where `T` is the Rust type for the item schema (string → `String`, integer → `i64`, number → `f64`, object with properties → struct name, string enum → enum name, array with items → `Vec<Inner>`). Unsupported item types (e.g. `null`, `boolean` only) emit `Vec<serde_json::Value>`. The validator, when `type_ == "array"` and `items` is present, validates each array element against the item schema (iterative stack, no recursion). Reverse codegen: `Vec<T>::json_schema()` returns `type: "array"` and `items: Some(Box::new(T::json_schema()))`. We support minItems, maxItems, and uniqueItems (see their subsections). prefixItems, contains, unevaluatedItems, etc. are not implemented; see the TODO subsections below.

**Spec version quirks:**

- **Draft-00, 01, 02:** `items` is not in the core spec in the same form; later drafts standardize it.
- **Draft-03, 04:** `items` may be a schema (object) or an array of schemas. When it is an object, all elements must validate against that schema. We support only the single-schema form.
- **Draft 2019-09, 2020-12:** `prefixItems` is an array of schemas for tuple typing; `items` applies to indices beyond the prefixItems length (or all indices if prefixItems is absent). We support only `items` as a single schema; we ignore `prefixItems`. For compatibility, a schema with only `items` (single schema) validates all array elements against that schema, matching draft-04 behavior.

### prefixItems

TODO.

**Spec version quirks:** (placeholder or blank)

### contains / minContains / maxContains

TODO.

**Spec version quirks:** (placeholder or blank)

### minItems / maxItems

`minItems` and `maxItems` constrain array length: valid if `size >= minItems` and `size <= maxItems`. Apply only when the instance is an array (after type check). Same meaning across draft-04 through 2020-12.

**Our implementation:** We parse and store `min_items` and `max_items` as `Option<u64>`. **Validator:** When `type_ == "array"` and the instance is an array, we check length against `min_items` (if present) and `max_items` (if present); we push `ValidationError::TooFewItems` or `TooManyItems` and continue (no fail-fast). Length bounds apply to all arrays regardless of `uniqueItems` (i.e. both Vec and HashSet in codegen). **Codegen:** Emitted Rust type remains `Vec<T>` or `HashSet<T>` (and optional variants) per existing rules; when the schema has `min_items` or `max_items`, we emit `#[to_json_schema(min_items = N, max_items = M)]` on the generated field (only the keys that are present) so generated models round-trip and reverse codegen reproduces the schema. **DedupeKey** includes `min_items` and `max_items` so array/set schemas with different bounds are not deduplicated. **Reverse codegen:** `Vec<T>::json_schema()` and `HashSet<T>::json_schema()` set `min_items: None`, `max_items: None`; the derive supports `#[to_json_schema(min_items = N, max_items = M)]` on array and set fields (Vec, HashSet, Option<Vec>, Option<HashSet>) and overlays them on the property schema.

**Spec version quirks:** Draft-04 meta uses positiveInteger for `maxItems` and positiveIntegerDefault0 for `minItems`; 2019-09 and 2020-12 use nonNegativeInteger / nonNegativeIntegerDefault0. Validation semantics are identical; we use `u64` and accept 0 for both where the spec allows.

### uniqueItems

When `uniqueItems` is `true`, all array elements must be unique (JSON structural equality). Default/absent = false. Meaning is the same across draft-02 through 2020-12.

**Our implementation:** We parse and store `unique_items` as `Option<bool>`. **Validator:** When `type: "array"` and `unique_items == Some(true)`, we check that no two elements are equal (using `serde_json::Value` equality); if a duplicate is found we push `ValidationError::DuplicateArrayItems` and continue. **Codegen:** When `unique_items == Some(true)` and the item type is hashable (string, integer, number, or string enum), we emit `HashSet<T>` or `Option<HashSet<T>>`; otherwise we emit `Vec<T>` and the validator enforces uniqueness. **Reverse codegen:** `Vec<T>::json_schema()` does not set `unique_items` (omit = false). `HashSet<T>::json_schema()` returns `type: "array"`, `items: T::json_schema()`, and `unique_items: Some(true)`.

**Spec version quirks:** None; the keyword is boolean with the same meaning (all elements unique when true) across draft-02 through 2020-12.

### unevaluatedItems

TODO.

**Spec version quirks:** (placeholder or blank)

---

## 7. Strings

### Strings (type: "string")

When a schema has `"type": "string"` (or `"type": ["string", ...]` with string first), the instance must be a JSON string. This meaning is **identical across all JSON Schema drafts** (draft-00 through 2020-12).

**Our implementation:** We parse both a single type string `"string"` and an array whose first element is `"string"` (we store the first type only; see **4. type**). Validation: when `type_ == Some("string")`, we require the instance to be a JSON string (`instance.is_string()`); non-strings produce `ValidationError::ExpectedString`. String length constraints (`minLength`, `maxLength`) are applied when the instance is a string (see **minLength / maxLength** below). Codegen: properties with `type: "string"` emit Rust `String` or `Option<String>` (with `#[serde(rename = "...")]` when the field name differs from the JSON key; `#[to_json_schema(min_length = N, max_length = N)]` when constraints are present). See schema model `is_string()` and parsing in `json_schema_rs/src/json_schema/json_schema.rs`, validator in `json_schema_rs/src/validator/mod.rs`, and Rust backend in `json_schema_rs/src/code_gen/rust_backend.rs`. Reverse codegen: Rust `String` emits `"type": "string"` with no length bounds. Field-level `#[to_json_schema(min_length = N, max_length = N)]` attributes on String fields wire constraints through the derive macro.

**Spec version quirks:** None for the **meaning** of type `"string"` (instance must be a string). The only draft differences are in the **form** of the `type` keyword (string vs array, and draft-03 array may contain schema objects), documented under **4. Type and value constraints → type**.

### minLength / maxLength

`minLength` and `maxLength` constrain the length of string instances. The length is counted as Unicode code points (`.chars().count()`), not UTF-8 bytes. Both keywords are string-only; they are ignored when the instance is not a string (the `ExpectedString` error covers that case). Both are inclusive: `minLength: 3` means the string must have at least 3 code points; `maxLength: 50` means at most 50.

**Our implementation:**
- **Schema model:** `JsonSchema.min_length: Option<u64>` and `JsonSchema.max_length: Option<u64>` (serialized as `"minLength"` / `"maxLength"`). Both `None` by default; omitted from serialized JSON when absent.
- **Validator:** In the `Some("string")` arm, after the type check, if the instance is a string, `s.chars().count()` is compared against `min_length` and `max_length`. Violations emit `ValidationError::TooShort { min_length }` or `ValidationError::TooLong { max_length }`. These are only emitted when the instance IS a string; non-string instances get only `ExpectedString`.
- **Codegen (JSON Schema → Rust):** When a string property schema has `min_length` and/or `max_length`, the backend emits `#[to_json_schema(min_length = N)]`, `#[to_json_schema(max_length = N)]`, or `#[to_json_schema(min_length = N, max_length = N)]` before the field declaration. This is consistent with the `min_items`/`max_items` pattern for array fields.
- **Reverse codegen (Rust → JSON Schema):** `String::json_schema()` returns `"type": "string"` with no length constraints. Field-level `#[to_json_schema(min_length = N, max_length = N)]` on a `String` (or `Option<String>`) field feeds into `property_inserts` in the derive macro, producing a schema with the specified `minLength`/`maxLength`. The `.or(base.min_length)` / `.or(base.max_length)` merge means field attributes take precedence over base type defaults (which are `None`).
- **Macro (`json_schema_to_rust!`):** The macro generates Rust source with the `#[to_json_schema(...)]` attributes on string fields; when the generated code is compiled, the `ToJsonSchema` derive re-derives the schema (round-trip verified in the test suite).

**Spec version quirks:** Present from Draft 3 onward. In Draft 3 and 4, `minLength` defaults to 0 (no minimum) and `maxLength` has no default (unconstrained). The character-counting semantics (`chars`, i.e., Unicode code points / UCS-2 code units in earlier drafts) have been stable. In practice, counting `.chars()` (Rust Unicode scalar values) matches the spec's intent for all common text.

#### pattern

TODO (string-only constraint).

**Spec version quirks:** (placeholder or blank)

#### contentEncoding / contentMediaType / contentSchema

TODO.

**Spec version quirks:** (placeholder or blank)

---

## 8. Numbers (integer / number)

### type: "integer"

When a schema has `"type": "integer"` (or `"type": ["integer", ...]` with integer first), the instance must be a JSON number with no fractional part (a mathematical integer). This meaning is **identical across all JSON Schema drafts** (draft-00 through 2020-12).

**Our implementation:** We parse both a single type string `"integer"` and an array whose first element is `"integer"` (we store the first type only; see **4. type**). Validation: when `type_ == Some("integer")`, we require the instance to be a JSON number that is an integer (e.g. `instance.as_number().is_some_and(|n| n.as_i64().is_some())`); non-integers (float, string, null, etc.) produce `ValidationError::ExpectedInteger`. When the instance is numeric, we apply `minimum` and `maximum` if present (see **minimum** / **maximum**). Codegen: properties with `type: "integer"` emit a Rust integer type chosen from `minimum` and `maximum` when both present and valid, else `i64` or `Option<i64>` (see **minimum** / **maximum**). Reverse codegen: Rust types `i8`, `u8`, `i16`, `u16`, `i32`, `u32`, `i64`, `u64` all emit `"type": "integer"` with `minimum` and `maximum` set to the type's range. See schema model `is_integer()` in `json_schema_rs/src/json_schema/json_schema.rs`, validator in `json_schema_rs/src/validator/mod.rs`, Rust backend in `json_schema_rs/src/code_gen/rust_backend.rs`, and reverse codegen in `json_schema_rs/src/reverse_code_gen/mod.rs`.

**Spec version quirks:** None for the **meaning** of type `"integer"` across drafts. The only draft differences are in the **form** of the `type` keyword (string vs array), documented under **4. Type and value constraints → type**.

### type: "number"

When a schema has `"type": "number"` (or `"type": ["number", ...]` with number first), the instance must be a JSON number (integer or floating-point). This meaning is **identical across all JSON Schema drafts** (draft-00 through 2020-12).

**Our implementation:** We parse both a single type string `"number"` and an array whose first element is `"number"` (we store the first type only; see **4. type**). Validation: when `type_ == Some("number")`, we require the instance to be a JSON number (`instance.as_number().is_some()`); non-numbers (string, null, object, array, boolean) produce `ValidationError::ExpectedNumber`. When the instance is numeric, we apply `minimum` and `maximum` if present (see **minimum** / **maximum**). Codegen: properties with `type: "number"` emit Rust `f32` or `f64` (or Option) chosen from `minimum` and `maximum` when both present and valid, else `f64` (see **minimum** / **maximum**). Reverse codegen: Rust types `f32` and `f64` emit `"type": "number"` with `minimum` and `maximum` set to the type's range. See schema model `is_number()` in `json_schema_rs/src/json_schema/json_schema.rs`, validator in `json_schema_rs/src/validator/mod.rs`, Rust backend in `json_schema_rs/src/code_gen/rust_backend.rs`, and reverse codegen in `json_schema_rs/src/reverse_code_gen/mod.rs`.

**Spec version quirks:** None for the **meaning** of type `"number"` across drafts. The only draft differences are in the **form** of the `type` keyword (string vs array), documented under **4. Type and value constraints → type**.

### minimum

The value of `minimum` MUST be a number, representing an inclusive lower limit for a numeric instance. If the instance is a number, it validates only if the instance is greater than or exactly equal to `minimum`. This meaning is consistent across draft-04 through 2020-12 (draft-00/01 had optional `minimumCanEqual`, default true, i.e. inclusive).

**Our implementation:** We store `minimum` as `Option<f64>` in the schema model (see `json_schema_rs/src/json_schema/json_schema.rs`). Validation: when the schema has `type: "integer"` or `type: "number"` and the instance is numeric, we require instance ≥ `minimum` when `minimum` is present; otherwise we push `ValidationError::BelowMinimum`. Codegen: we use `minimum` together with `maximum` when both are present and valid to choose the smallest Rust integer or float type that fits the range; if either is absent or invalid (e.g. min > max, or for integer non-integer or out of i64 range), we fall back to `i64` for integer and `f64` for number. Reverse codegen: we emit `minimum` (and `maximum`) from the Rust type's range (e.g. i8 → -128..=127, u8 → 0..=255, f32 → f32::MIN..=f32::MAX). The derive macro allows overriding the type-derived bound via field attribute **`#[to_json_schema(minimum = N)]`** (N an integer or float literal).

**Spec version quirks:** None for inclusive `minimum` in draft-04 through 2020-12. Draft-00 and draft-01 used `minimumCanEqual` (boolean, default true) to allow equality; we implement only the inclusive semantics.

### maximum

The value of `maximum` MUST be a number, representing an inclusive upper limit for a numeric instance. If the instance is a number, it validates only if the instance is less than or exactly equal to `maximum`. This meaning is consistent across draft-04 through 2020-12 (draft-00/01 had optional `maximumCanEqual`, default true, i.e. inclusive).

**Our implementation:** We store `maximum` as `Option<f64>` in the schema model. Validation: when the schema has `type: "integer"` or `type: "number"` and the instance is numeric, we require instance ≤ `maximum` when `maximum` is present; otherwise we push `ValidationError::AboveMaximum`. Codegen: used together with `minimum` for range-based type selection (see **minimum**). Reverse codegen: we emit `maximum` from the Rust type's range. The derive macro allows overriding the type-derived bound via field attribute **`#[to_json_schema(maximum = N)]`** (N an integer or float literal).

**Spec version quirks:** None for inclusive `maximum` in draft-04 through 2020-12. Draft-00/01 used `maximumCanEqual` (boolean, default true); we implement only the inclusive semantics.

### exclusiveMinimum

TODO (number/integer).

**Spec version quirks:** (placeholder or blank)

### exclusiveMaximum

TODO (number/integer).

**Spec version quirks:** (placeholder or blank)

### multipleOf

TODO (number/integer).

**Spec version quirks:** (placeholder or blank)

---

## 9. Metadata / annotations

### title

We use `title` for struct naming (PascalCase) when **model name source** is default (title first). If missing or empty, the root struct is named `Root` and nested structs are named from the property key (e.g. `address` → `Address`). The primary source (title vs property key) is **configurable**; see **Model name source** below.

**Spec version quirks:** (placeholder or blank)

#### Model name source (configurable)

**Default:** `TitleFirst` — struct/type name from `title` if non-empty, else property key, else `"Root"` (root only). **Option:** `PropertyKeyFirst` — property key first, then `title`, then `"Root"`. Set via [`RustCodegenOptions::with_property_key_first`], library [`generate_rust_with_options`], or CLI `--struct-name-from property-key`. Macro uses default (title first).

**Competitor summary:** Most codegen libraries use title as primary, then fallback to ref fragment / $id / property name (e.g. Typify, quicktype, PHP wol-soft, Python datamodel-code-gen, Kotlin, C#). schemafy (Rust) does not use title; type names from property/parent+field only. go omissis and jsonschema2pojo make the choice configurable (e.g. `struct-name-from-title`, `isUseTitleAsClassname()`). We match that by offering both orders and defaulting to title first for backward compatibility.

**Spec version quirks:** (placeholder or blank)

### description

**Our implementation:** We treat empty or whitespace-only `description` as absent (no doc lines emitted). Multi-line description: one `///` doc line per non-empty trimmed line; no blank doc lines. **Placement:** object schema `description` → struct doc; enum schema (string enum) `description` → enum doc; property schema `description` → field doc. **Dedupe:** Functional mode excludes `description` from the dedupe key (same shape with different description yields one struct); Full mode includes it (different descriptions yield separate structs). **Reverse codegen:** Container `#[to_json_schema(description = "...")]` sets schema `description`; the macro also attempts to read `///` doc comments on struct/enum for description. Per-field description is merged into each property's schema (from attribute or doc when supported).

**Spec version quirks:** (placeholder or blank)

### default

TODO. (Planned: preserve JSON `null` in default (Absent vs Present(Value)); serde `Option<Value>` with null loses that distinction. Two strategies: `UseTypeDefault` → `#[serde(default)]` when schema value equals type Default; `Custom` → generated function + `#[serde(default = "fn")]`. Optional + null: use `#[serde(default)]`. Emission order: enums first, then default functions, then structs. Out of scope: object defaults, non-empty array defaults.)

**Spec version quirks:** (placeholder or blank)

### examples

TODO.

**Spec version quirks:** (placeholder or blank)

### deprecated

TODO.

**Spec version quirks:** (placeholder or blank)

### readOnly / writeOnly

TODO.

**Spec version quirks:** (placeholder or blank)

### format

The `format` annotation is stored on `JsonSchema` as `Option<String>`. Currently only `"uuid"` is
acted upon; other values are preserved through parse/serialize round-trips but have no effect on
validation or codegen.

**UUID feature (`uuid` feature flag)**

Enable with `json-schema-rs = { features = ["uuid"] }` (or `["dep:uuid"]` internally). Activating
the feature:

- **Validation:** string instances whose schema has `format: "uuid"` are validated with
  `uuid::Uuid::parse_str`. An `InvalidUuidFormat` error is emitted for strings that do not conform
  to the UUID format.
- **Codegen:** string properties with `format: "uuid"` emit `uuid::Uuid` (required) or
  `Option<uuid::Uuid>` (optional) instead of `String`/`Option<String>`. A `use uuid::Uuid;`
  statement is automatically prepended. Array item schemas with `format: "uuid"` likewise produce
  `Vec<Uuid>` etc.
- **Reverse codegen:** `uuid::Uuid` implements `ToJsonSchema`, returning
  `{"type":"string","format":"uuid"}`. `Option<Uuid>`, `Vec<Uuid>`, and `HashSet<Uuid>` are
  supported through the existing generic impls.
- **Macro:** the `json_schema_to_rust!` proc-macro and `#[derive(ToJsonSchema)]` both respect the
  `uuid` feature in the macro crate; enable with
  `json-schema-rs-macro = { features = ["uuid"] }`.

**Spec version quirks:** JSON Schema draft-07 onwards lists `format` as a vocabulary keyword; its
validation behaviour is opt-in (annotation only by default). This library treats `"uuid"` as a
validation-enforcing keyword when the `uuid` feature is enabled.
