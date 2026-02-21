# json-schema-rs design and architecture

This document is the **design and architecture knowledge bank** for the json-schema-rs crate. It describes how the library is designed, how each JSON Schema feature is (or will be) implemented, and how the JSON Schema specification defines each feature across draft versions.

- **One section per feature/keyword** (or sub-sections for related keywords). Research uses **only vendored specs** under `specs/` (draft-00 through 2020-12)—no reliance on the web.
- Related features are grouped (e.g. string constraints under Strings, number constraints under Numbers). Each section can have **Spec version quirks** sub-sections for differences between drafts; we implement per the latest supported spec and may expose version-based config where behavior differs.

Implemented keywords: type (object, string), properties, required, title. Other keywords are documented in the sections below. Unknown schema keywords and property types that do not map to generated code are ignored and do not cause an error.

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

**Supported keywords:** `type` (object, string), `required`, `properties` (recursive). Does not resolve `$ref` or `$defs`; additional properties are allowed. The validator reuses the same JsonSchema struct as codegen; one parse, one model. A compiled validator (e.g. tree of validator nodes) can be added for performance; the same schema model would be used.

### Official JSON Schema Test Suite

We run the [JSON Schema Test Suite](https://github.com/json-schema-org/JSON-Schema-Test-Suite) via an integration test that validates against all test data in the suite. The suite lives at **`research/json-schema-test-suite/`** (gitignored). Cloning is a **manual prerequisite**: run `make vendor_test_suite` to clone or update it. The test **hard-fails** if the suite directory is missing, with a message to run that command. The test is **ignored** by default and runs only when explicitly executed (e.g. `make test_json_schema_suite` or `cargo test --test json_schema_test_suite -- --ignored`); once we pass 100%, it can be re-enabled in the standard test run. Tests that rely on **remotes** or **`$ref`** resolution (e.g. `refRemote.json`) fail until we support `$ref`.

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

**JsonSchemaSettings** control how JSON Schema definitions are ingested (parsed). Use `JsonSchemaSettings::builder()` to construct; options include `disallow_unknown_fields` (when `true`, reject schema objects that contain keys other than `type`, `properties`, `required`, `title`). Default is per-option. **SpecVersion** is an enum with one variant per vendored spec (Draft00 through Draft202012); `default_schema_settings()` returns a `JsonSchemaSettings` tuned for that spec. Schema ingestion uses **parse_schema** / **parse_schema_from_slice** with `&JsonSchemaSettings`; the CLI and macro build settings from flags or builders and pass them through. **CodeGenSettings** are language-agnostic codegen options (e.g. `model_name_source`). Use `CodeGenSettings::builder()`. The **CodeGenBackend** trait (in `code_gen/mod.rs`) takes `&CodeGenSettings` in `generate`. The Rust backend and `generate_rust` live in `code_gen/rust_backend.rs`; settings live in `code_gen/settings.rs`. CLI prefixes: `jss-` (JSON Schema Settings), `cgs-` (codegen settings); future Rust-specific: `cgs-rs-`.

### Codegen backends

Codegen is built around a **swappable backend** trait in **`code_gen/mod.rs`**: input is a slice of `JsonSchema`, **CodeGenSettings**, and output is one generated source buffer per schema. The trait `CodeGenBackend` has a single method, `generate(&self, schemas: &[JsonSchema], settings: &CodeGenSettings) -> Result<Vec<Vec<u8>>, Error>`. The **CLI** builds `CodeGenSettings` and `JsonSchemaSettings` from flags and calls the corresponding backend. The only implementation today is **Rust** (`RustBackend` in **`code_gen/rust_backend.rs`**), which emits serde-compatible Rust structs (one buffer per schema; no cross-schema dedupe yet). The public API is `generate_rust(schemas: &[JsonSchema], settings: &CodeGenSettings) -> Result<Vec<Vec<u8>>, Error>`; callers must pass settings (e.g. `CodeGenSettings::builder().build()` for all defaults). **Adding another language:** implement `CodeGenBackend` for a new type (e.g. `PythonBackend`) in a new module (e.g. `code_gen/python_backend.rs`), add a match arm in the CLI’s `run_generate` for the language name (case-insensitive), and update the "supported" text in the unsupported-language error message (e.g. "supported: rust, python").

**Codegen entry points:** (1) **CLI** — one or more INPUTs (file paths, directory paths recursively searched for `.json`, or `-` for stdin); required `-o` output directory. CLI flags: `--jss-disallow-unknown-fields`, `--cgs-model-name-source title-first|property-key`. Schema file ingestion uses `parse_schema_from_slice` with `JsonSchemaSettings` built from flags; each failure is logged to stderr and the command exits with failure after all files have been attempted (no output is written if any ingestion fails). Generated output path components are **sanitized** so that file and directory names are valid Rust identifiers. A `mod.rs` is emitted in each output directory. (2) **Library** — `generate_rust(schemas, &CodeGenSettings::builder().build())` returns `Vec<Vec<u8>>`; one buffer per schema. Parse schemas with `parse_schema(json, &JsonSchemaSettings::builder().build())` or `parse_schema_from_slice`. (3) **Macro** — the `json-schema-rs-macro` crate provides `json_schema_to_rust!(...)`, which runs at compile time and **inlines** the generated Rust at the call site. Use `json_schema_rs_macro::json_schema_to_rust`. The macro builds `JsonSchemaSettings` and `CodeGenSettings` via their builders (no args) and calls `RustBackend::generate(&schemas, &code_gen_settings)`; module name from file stem for paths, or `schema_0`, `schema_1`, … for inline. Macro-related tests and fixtures live in the macro crate; the main crate has no macro-specific tests. Consumers add both `json-schema-rs` and `json-schema-rs-macro` and use `json_schema_rs_macro::json_schema_to_rust`. A re-export from the main crate would require the main crate to depend on the macro crate, which would create a cyclic workspace dependency (the macro crate depends on the main crate for codegen).

**Future: shared module / dedupe.** The multi-schema API and CLI output layout are designed so that cross-schema deduplication can later emit a shared module (e.g. `shared.rs`) and per-schema modules that use it; no change to CLI arguments.

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

TODO.

**Spec version quirks:** (placeholder or blank)

### $id

TODO.

**Spec version quirks:** (placeholder or blank)

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

TODO.

**Spec version quirks:** (placeholder or blank)

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

TODO.

**Spec version quirks:** (placeholder or blank)

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

We support a single type string or an array of types (draft 2020-12 style); we take the **first** type. `object` and `string` drive codegen today; other types are ignored. See schema model in `json_schema_rs/src/json_schema/json_schema.rs` and Rust codegen in `json_schema_rs/src/code_gen/rust_backend.rs`.

**Spec version quirks:** (placeholder or blank)

### const

TODO.

**Spec version quirks:** (placeholder or blank)

### enum

TODO. (Planned: JSON Schema `enum` = array of allowed values. Only string enums supported; non-string values fall back to `String`. Variant naming: PascalCase; invalid identifiers get `E` prefix. Collision handling: when multiple JSON values map to same Rust variant name, append `_0`, `_1`, `_2` to **all** colliding variants. Determinism: sort enum values alphabetically. Deduplication: duplicate JSON values → one variant. In tests, use `r#"..."#` when expected output contains `]"`.)

**Spec version quirks:** (placeholder or blank)

---

## 5. Objects

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

### items

TODO.

**Spec version quirks:** (placeholder or blank)

### prefixItems

TODO.

**Spec version quirks:** (placeholder or blank)

### contains / minContains / maxContains

TODO.

**Spec version quirks:** (placeholder or blank)

### minItems / maxItems

TODO.

**Spec version quirks:** (placeholder or blank)

### uniqueItems

TODO.

**Spec version quirks:** (placeholder or blank)

### unevaluatedItems

TODO.

**Spec version quirks:** (placeholder or blank)

---

## 7. Strings

### Strings (type: "string")

We emit properties with `type: "string"` as `String`. No string validation keywords (minLength, maxLength, pattern) are implemented yet.

**Spec version quirks:** (placeholder or blank)

#### minLength

TODO (string-only constraint).

**Spec version quirks:** (placeholder or blank)

#### maxLength

TODO (string-only constraint).

**Spec version quirks:** (placeholder or blank)

#### pattern

TODO (string-only constraint).

**Spec version quirks:** (placeholder or blank)

#### contentEncoding / contentMediaType / contentSchema

TODO.

**Spec version quirks:** (placeholder or blank)

---

## 8. Numbers (integer / number)

### minimum

TODO. (Planned: we use `minimum` and `maximum` when both present and valid to choose the smallest integer or float type. Fallback: no/min/max or invalid → `i64` for integer, `f64` for number. No validation; type selection only.)

**Spec version quirks:** (placeholder or blank)

### maximum

TODO. (Planned: see minimum; used together for range-based type selection.)

**Spec version quirks:** (placeholder or blank)

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

TODO. (Planned: empty or whitespace-only `description` treated as absent; no blank doc lines. Multi-line: one doc line per line of text. Placement: object schema `description` → struct doc; enum schema → enum doc; property schema → field doc.)

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

TODO.

**Spec version quirks:** (placeholder or blank)
