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

We have three separate pipelines: Schema→Rust, Rust→Schema, and the validator. Code layout: workspace crates `json_schema_rs/` (lib, core logic) and `json_schema_to_rust_cli/` (CLI). When adding a new keyword or type, consider: schema model, codegen/validation behavior, tests, and examples.

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

We support a single type string or an array of types (draft 2020-12 style); we take the **first** type. `object` and `string` drive codegen today; other types are ignored. See schema model in `json_schema_rs/src/schema.rs` and codegen in `json_schema_rs/src/codegen.rs`.

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

We use `properties` to build structs: each property becomes a struct field. Property keys are sanitized for Rust (e.g. `-` → `_`). When the Rust field name differs from the JSON key, we emit `#[serde(rename = "...")]`. Object schemas are traversed recursively; each object with `properties` yields a Rust struct. See `schema.rs` and `codegen.rs`.

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

We use `title` for struct naming (PascalCase). If missing or empty, the root struct is named `Root` and nested structs are named from the property key (e.g. `address` → `Address`).

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
