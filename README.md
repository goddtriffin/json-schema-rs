# json-schema-rs

[![Version](https://img.shields.io/crates/v/json-schema-rs)](https://crates.io/crates/json-schema-rs)
[![Docs](https://docs.rs/json-schema-rs/badge.svg)](https://docs.rs/json-schema-rs)

A Rust library to generate Rust structs from JSON Schema.

## Features

- **Unsupported keys are ignored.** Unknown keywords and unsupported types do
  not cause an error; they are skipped.

### Objects

The root schema must have `type: "object"`. Object schemas are traversed
recursively; each object with `properties` yields a Rust struct. Non-object
types at the root cause generation to fail.

The `properties` key defines the shape of each object. Each property becomes a
struct field. Property keys are sanitized for Rust (e.g. `-` becomes `_`); when
the Rust field name differs from the JSON key, generated code includes
`#[serde(rename = "...")]`.

Nested `properties` produce nested structs. Child structs are emitted before
parent structs (topological order), so the generated Rust compiles without
reordering.

The `title` of an object schema is used as the struct name (PascalCase). If
`title` is missing or empty, the struct name is derived from the property key
that references it.

### Strings

Properties with `type: "string"` are emitted as `String`.

### Enums

When a property has an `enum` of string values, generates a Rust enum instead of
`String`. Variant names are PascalCase. If multiple JSON values map to the same
variant name (e.g. `"PENDING"` and `"pending"`), suffixes like `_0`, `_1` are
applied so variant names stay unique.

### Booleans

Properties with `type: "boolean"` are emitted as `bool`.

### Numbers

Properties with `type: "integer"` are emitted as the smallest Rust integer type
that fits the range: `i8`, `u8`, `i16`, `u16`, `i32`, `u32`, `i64`, or `u64`.
When `minimum` and `maximum` are both present and valid integers, the generator
picks the smallest type that can hold the range; otherwise it uses `i64`.

Properties with `type: "number"` are emitted as `f32` when both `minimum` and
`maximum` are present and within f32 range (approximately ±3.4e38); otherwise
`f64`. No validation is performed—min/max are used only for type selection.
Float selection is range-based only; `f32` may lose precision for some decimal
values.

### Arrays

Properties with `type: "array"` and an `items` schema are emitted as `Vec<T>` or
`Option<Vec<T>>`. If `items` is missing or has an unsupported type, the property
is skipped.

### Required vs Optional

The `required` array lists property names that are required at that object
level. Required properties are emitted as `T`; others as `Option<T>`. If
`required` is absent, all properties are treated as optional.

The non-standard per-property `optional` keyword is recognized but **ignored**;
required vs optional is determined only by the `required` array. Future versions
may offer strict spec adherence or options for non-standard keywords.

### default

Properties with a `default` value get `#[serde(default)]` or
`#[serde(default =
"fn")]` so missing JSON keys use the default when
deserializing:

- When the default equals the type's `Default` (e.g. `false` for bool, `0` for
  integer, `""` for string, `[]` for array, `null` for optional), emits
  `#[serde(default)]`.
- Otherwise, generates a module-level function and emits
  `#[serde(default = "default_StructName_field")]`.

Supported defaults: `boolean`, `integer`, `number`, `string`, string `enum`, and
empty array `[]`. Object defaults and non-empty array defaults are not
supported.

### additionalProperties

The `additionalProperties` keyword controls extra keys on an object:

- **`additionalProperties: false`** — No extra keys allowed. The generated
  struct gets `#[serde(deny_unknown_fields)]`.
- **`additionalProperties: true`** or absent — Extra keys are allowed and
  ignored (default serde behavior).
- **`additionalProperties: { "type": "string" }`** (or another schema) — Extra
  keys are captured in a flattened `BTreeMap<String, T>` field
  `additional_properties`, where `T` is the type from the schema.

### description

The `description` keyword is emitted as Rust `///` doc comments. Empty or
whitespace-only descriptions are omitted.

### Unsupported features

| Feature                                                | Description                                                                                                        |
| ------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------ |
| `$ref` / `definitions` / `$defs`                       | Schema reuse and shared types                                                                                      |
| `minLength` / `maxLength` / `pattern`                  | String validation or custom deserialization                                                                        |
| `format` (e.g. `uuid4`)                                | Would generate `Uuid` or validation                                                                                |
| `oneOf` / `anyOf` / `allOf`                            | Composition; enum or flattened structs                                                                             |
| `optional`                                             | Recognized but ignored; required/optional from `required` only. Future: strict mode or options to allow/interpret. |
| `$id`                                                  | Schema identification/referencing                                                                                  |
| `examples`                                             | Documentation/tests                                                                                                |
| `const`                                                | Single allowed value                                                                                               |
| `not`                                                  | Exclusion                                                                                                          |
| `minProperties` / `maxProperties`                      | Object size constraints                                                                                            |
| `minItems` / `maxItems` / `uniqueItems`                | Array constraints                                                                                                  |
| `exclusiveMinimum` / `exclusiveMaximum` / `multipleOf` | Number constraints                                                                                                 |
| `readOnly` / `writeOnly` / `deprecated`                | Metadata                                                                                                           |
| `propertyNames` / `additionalItems`                    | Object/array constraints                                                                                           |
| `null` type / type array                               | Multiple types                                                                                                     |

## Examples

JSON Schema:

```json
{
  "type": "object",
  "title": "Record",
  "description": "A record with id and optional fields.",
  "required": ["id"],
  "additionalProperties": { "type": "string" },
  "properties": {
    "active": { "type": "boolean" },
    "count": { "type": "integer", "minimum": 0, "maximum": 255 },
    "id": { "type": "string", "description": "Unique identifier." },
    "name": { "type": "string" },
    "score": { "type": "number", "minimum": 0, "maximum": 1 },
    "status": {
      "type": "string",
      "enum": ["active", "inactive"],
      "default": "active",
      "description": "Current status."
    },
    "nested": {
      "type": "object",
      "title": "NestedInfo",
      "required": ["value"],
      "properties": {
        "value": { "type": "string" },
        "kind": { "type": "string", "enum": ["A", "a"] }
      }
    },
    "foo-bar": { "type": "string" },
    "tags": { "type": "array", "items": { "type": "string" } }
  }
}
```

Generated Rust:

```rust
//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum Kind {
    #[serde(rename = "A")]
    A_0,
    #[serde(rename = "a")]
    A_1,
}

/// Current status.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum Status {
    #[serde(rename = "active")]
    Active,
    #[serde(rename = "inactive")]
    Inactive,
}

fn default_Record_status() -> Option<Status> {
    Some(Status::Active)
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NestedInfo {
    pub kind: Option<Kind>,
    pub value: String,
}

/// A record with id and optional fields.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Record {
    #[serde(flatten)]
    pub additional_properties: BTreeMap<String, String>,
    pub active: Option<bool>,
    pub count: Option<u8>,
    #[serde(rename = "foo-bar")]
    pub foo_bar: Option<String>,
    /// Unique identifier.
    pub id: String,
    pub name: Option<String>,
    pub nested: Option<NestedInfo>,
    pub score: Option<f32>,
    /// Current status.
    #[serde(default = "default_Record_status")]
    pub status: Option<Status>,
    pub tags: Option<Vec<String>>,
}
```

[View full example in `examples/readme_example.rs`](examples/readme_example.rs)

## Running the binary

The crate includes a CLI binary `json-schema-gen` that reads a JSON Schema from
stdin and writes generated Rust code to stdout.

**Build the binary:**

```bash
cargo build --release
```

The binary is at `target/release/json-schema-gen`.

**Run it:**

```bash
json-schema-gen < schema.json > output.rs
```

Or pipe from another command:

```bash
cat schema.json | json-schema-gen > output.rs
```

## Alternative libraries

TODO

## Developers

**Project is under active maintenance - even if there are no recent commits!
Please submit an issue / bug request if the library needs updating for any
reason!**

### Philosophy

- Generates idiomatic Rust
- Can handle every JSON Schema specification version

### Commands

- `make lint`
- `make test`
- `make fix`

## Credits

Made by [Todd Everett Griffin](https://www.toddgriffin.me/).
