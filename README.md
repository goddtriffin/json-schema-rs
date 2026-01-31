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

Properties with `type: "integer"` are emitted as `i64`. Properties with
`type: "number"` are emitted as `f64`.

### Arrays

Properties with `type: "array"` and an `items` schema are emitted as `Vec<T>` or
`Option<Vec<T>>`. Supported item types: `string` → `String`, `boolean` → `bool`,
`integer` → `i64`, `number` → `f64`, `object` → a nested struct, and string
`enum` → a Rust enum. If `items` is missing or has an unsupported type, the
property is skipped.

### Required vs Optional

The `required` array lists property names that are required at that object
level. Required properties are emitted as `T`; others as `Option<T>`. If
`required` is absent, all properties are treated as optional.

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
  `additional_properties`, where `T` is the type from the schema (`String`,
  `i64`, `serde_json::Value`, a nested struct, etc.). The generated code
  includes `use std::collections::BTreeMap;` when this is used.

### description

The `description` keyword is emitted as Rust `///` doc comments. Empty or
whitespace-only descriptions are omitted.

### Unsupported features

| Feature                                                | Description                                       |
| ------------------------------------------------------ | ------------------------------------------------- |
| `$ref` / `definitions` / `$defs`                       | Schema reuse and shared types                     |
| `minLength` / `maxLength` / `pattern`                  | String validation or custom deserialization       |
| `format` (e.g. `uuid4`)                                | Would generate `Uuid` or validation               |
| `oneOf` / `anyOf` / `allOf`                            | Composition; enum or flattened structs            |
| `optional`                                             | Non-standard; similar to omitting from `required` |
| `$id`                                                  | Schema identification/referencing                 |
| `examples`                                             | Documentation/tests                               |
| `const`                                                | Single allowed value                              |
| `not`                                                  | Exclusion                                         |
| `minProperties` / `maxProperties`                      | Object size constraints                           |
| `minItems` / `maxItems` / `uniqueItems`                | Array constraints                                 |
| `exclusiveMinimum` / `exclusiveMaximum` / `multipleOf` | Number constraints                                |
| `readOnly` / `writeOnly` / `deprecated`                | Metadata                                          |
| `propertyNames` / `additionalItems`                    | Object/array constraints                          |
| `minimum` / `maximum`                                  | Number bounds                                     |
| `null` type / type array                               | Multiple types                                    |

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
    "count": { "type": "integer" },
    "id": { "type": "string", "description": "Unique identifier." },
    "name": { "type": "string" },
    "score": { "type": "number" },
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
    pub count: Option<i64>,
    #[serde(rename = "foo-bar")]
    pub foo_bar: Option<String>,
    /// Unique identifier.
    pub id: String,
    pub name: Option<String>,
    pub nested: Option<NestedInfo>,
    pub score: Option<f64>,
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
