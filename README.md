# json-schema-rs

[![Version](https://img.shields.io/crates/v/json-schema-rs)](https://crates.io/crates/json-schema-rs)
[![Docs](https://docs.rs/json-schema-rs/badge.svg)](https://docs.rs/json-schema-rs)

A Rust library for JSON Schema tooling: **Schema→Rust** codegen (generate Rust
types from a JSON Schema), **Rust→Schema** reverse codegen, and a **validator**.
The repo provides the `json-schema-rs` library and the `jsonschemars` CLI. We
target **JSON Schema Draft 2020-12** and follow each document's `$schema` for
drafts 4, 6, 7, 2019-09 and 2020-12 (Draft 2020-12 when `$schema` is absent);
`JsonSchemaSettings` can pin a draft or override one behaviour. A script downloads
specs for every published draft (draft-00 through 2020-12); run
`make vendor_specs` to fetch them locally—specs are not stored in the repo.
Supported keywords include **type** (all seven types, a single name or an array of
names; the validator accepts any listed type and rejects unknown names at parse time; codegen
reads `["T", "null"]` as `T` and omits a property with several non-null types, as it does a typeless one), **boolean
schemas** (`true` / `false` anywhere a schema may appear, including a whole document; parse one
with `Schema::new_from_str`), **properties**, **required**, **const** (draft-06+;
validator: instance must equal const, numbers by value so `1` equals `1.0`; codegen: string const → single-value enum, non-string const → fallback; reverse: single-variant unit enum → const), **enum**
(validator compares numbers by value; codegen emits Rust enums for string-only enums), **items** (array with single-schema
items; codegen emits `Vec<T>` or `Option<Vec<T>>`; **uniqueItems**: when true,
codegen emits `HashSet<T>` for hashable item types and the validator enforces
uniqueness, with `1` and `1.0` equal and object key order ignored), **minItems** and **maxItems** (array/set length constraints;
validator enforces; codegen emits
`#[json_schema(min_items = ..., max_items = ...)]` on generated array/set
fields; reverse codegen supports the same attributes on Vec and HashSet fields),
**minimum**, **maximum**, **exclusiveMinimum**, **exclusiveMaximum** and
**multipleOf** (held and compared exactly, with no rounding or epsilon:
`"minimum": 9007199254740993` rejects `9007199254740992`, `0.0075` is a multiple
of `0.0001`; the bounds choose narrow integer/float types in codegen; draft 4's
boolean `exclusiveMinimum` is read as draft 6's numeric form), **minLength** and
**maxLength** (string length constraints in Unicode code points; validator
enforces; codegen emits `#[json_schema(min_length = ..., max_length = ...)]`
on generated string fields; reverse codegen supports the same attributes on
String fields), **pattern** (string constraint as ECMA 262 regex; validator
enforces via regress; codegen emits `#[json_schema(pattern = "...")]`;
reverse codegen supports the same attribute on String fields), **default** (meta-data only; codegen emits `#[serde(default)]` or `#[serde(default = "fn")]` so missing keys get the schema default; reverse codegen via `#[json_schema(default = ...)]`), **description** (codegen emits Rust doc comments; reverse
codegen from `///` doc comments on structs, enums and fields, or `#[json_schema(description = "...")]`),
**examples** (draft-06+; meta-data only; stored and round-tripped; codegen emits examples in doc comments for structs and enums; not used for validation; Full dedupe includes in key, Functional excludes), **deprecated** (draft 2019-09+; meta-data only; codegen emits `#[deprecated]` on the corresponding field or struct; reverse codegen via `#[json_schema(deprecated = true)]`), **$comment** (draft-07+; stored and round-tripped; not used for validation; reverse codegen via `#[json_schema(comment = "...")]`), and **$schema**
(stored and round-tripped; selects the draft's rules, per schema resource, unless a draft is forced;
reverse codegen emits Draft 2020-12 URI by default; default is Draft 2020-12
when absent; an unrecognized URI falls back to Draft 2020-12 with a warning, or is
rejected with `UnknownSchemaUri::Reject`), and
**$id** (unique identifier; stored and round-tripped; a draft-4 document's `id` is read as `$id`; Full dedupe includes`
$id` in the key, Functional does not; reverse codegen via `#[json_schema(id = \"...\")]`; forward codegen emits the attribute when the schema has `$id`
so round-trip preserves it), **allOf** (validator: instance must validate
against every subschema; codegen: branches merged on-the-fly into one Rust
model; reverse codegen: not supported), **anyOf** (validator: instance must validate against at least one subschema; codegen: non-empty anyOf becomes a Rust enum with one variant per branch, including root anyOf; reverse codegen: not currently emitted), **oneOf** (validator: instance must validate against exactly one subschema; codegen: non-empty oneOf becomes a Rust enum with one variant per branch, including root oneOf; reverse codegen: not currently emitted), and **additionalProperties** (validator: false → one error per additional key; schema → validate each additional property value; codegen: false → `#[serde(deny_unknown_fields)]`, schema → `additional: BTreeMap<String, T>`; reverse: closed structs emit false, `BTreeMap<String, V>` emits object with additionalProperties). For implementation details and design
philosophy, see [design.md](design.md). Generated struct and field names are
always valid Rust identifiers (reserved words and invalid characters are
escaped; see design.md for sanitization rules).

## Example

JSON Schema:

```json
{
  "type": "object",
  "properties": {
    "first_name": { "type": "string" },
    "last_name": { "type": "string" },
    "birthday": { "type": "string" },
    "age": { "type": "integer" },
    "score": { "type": "number" },
    "address": {
      "type": "object",
      "properties": {
        "street_address": { "type": "string" },
        "city": { "type": "string" },
        "state": { "type": "string" },
        "country": { "type": "string" }
      }
    }
  }
}
```

Generated Rust:

```rust
//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Address {
    pub city: Option<String>,
    pub country: Option<String>,
    pub state: Option<String>,
    pub street_address: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub address: Option<Address>,
    pub age: Option<i64>,
    pub birthday: Option<String>,
    pub score: Option<f64>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
}
```

Every generated struct implements **ToJsonSchema** (e.g. `Root::json_schema()`
returns a `Result<JsonSchema, Vec<ReverseCodeGenError>>`). Serialize to JSON with `String::try_from(&schema)` or
`Vec::<u8>::try_from(&schema)`. Reverse codegen emits a flat root-level `$defs`
map with `$ref` for shared and recursive types. See [design.md](design.md) for
reverse codegen details. **`$ref`** resolves through a shared `Registry` for both
validation and Rust codegen: JSON Pointer fragments into any schema location (including
unknown keywords), `$id` / draft-4 `id` resources with RFC 3986 base-URI resolution,
`$anchor` / `$dynamicAnchor` (as plain anchors), URNs, the embedded standard
metaschemas, and other documents you supply (in memory or through a `Retrieve`
implementation; nothing is fetched by default). A custom `$schema` metaschema's
`$vocabulary` decides which keyword groups apply. `$dynamicRef` / `$recursiveRef`
are not resolved dynamically.

## Using the library

Add to your `Cargo.toml`:

```toml
[dependencies]
json-schema-rs = "0.0.4"
```

Parse one or more schemas and generate Rust (one buffer per schema, plus an
optional shared buffer when dedupe finds identical shapes). With default
settings you can use **TryFrom**:
`let schema = JsonSchema::try_from(schema_json)?;` or `schema_json.try_into()?`.
For custom settings, use the constructors:

```rust
use json_schema_rs::{JsonSchema, CodeGenSettings, DedupeMode, JsonSchemaSettings, ModelNameSource, generate_rust};
use std::io::Read;

// Default settings: use TryFrom (e.g. JsonSchema::try_from(schema_json_str)?). For custom settings:
let schema_settings = JsonSchemaSettings::default();

// From a string:
let json_schema = JsonSchema::new_from_str(schema_json_str, &schema_settings)?;
// From bytes (e.g. a buffer or file contents):
let json_schema = JsonSchema::new_from_slice(&bytes[..], &schema_settings)?;
// From an already-parsed serde_json::Value (avoids string round-trips):
let json_schema = JsonSchema::new_from_serde_value(&json_value, &schema_settings)?;
// From a reader (e.g. std::fs::File, std::io::Stdin):
let json_schema = JsonSchema::new_from_reader(reader, &schema_settings)?;
// From a file path:
let json_schema = JsonSchema::new_from_path("path/to/json-schema.json", &schema_settings)?;

// Then generate Rust:
let code_gen_settings = CodeGenSettings::builder().build();
let output = generate_rust(&[json_schema], &code_gen_settings)?;
// output.per_schema.len() == 1; write output.per_schema[0] to a file or stdout
// When dedupe finds shared structs, output.shared is Some(shared_rust_code)
```

Structurally identical object schemas are deduplicated by default (one generated
struct per shape, emitted in a shared buffer). Use
`CodeGenSettings::builder().dedupe_mode(DedupeMode::Disabled).build()` to turn
this off, or `DedupeMode::Functional` to compare only functional fields (see
[design.md](design.md)). Use
`JsonSchemaSettings::builder().disallow_unknown_fields(true).build()` to reject
schema definitions with keys their draft does not define (each such key is its
own error, naming the key and a JSON Pointer to the schema object that has it).
Parsing and `$ref` registry building report every problem they find, not just the
first (`JsonSchemaParseErrors`, `RegistryErrors`: one error per problem, one per
line when displayed). By default such keys, and
keywords this library does not implement, are kept in `JsonSchema::extensions`
and written back on serialization. Use
`CodeGenSettings::builder().model_name_source(ModelNameSource::PropertyKeyFirst).build()`
to prefer property keys over `title` for struct names. CLI:
`--jss-disallow-unknown-fields`, `--cgs-model-name-source title-first` or
`property-key`, `--cgs-dedupe-mode disabled|functional|full`.

**Drafts.** Behaviour that differs between drafts follows the document's draft by
default, so nothing needs configuring. `JsonSchemaSettings::builder()` can
`.default_spec_version(SpecVersion::Draft07)` (the draft for a document without
`$schema`, instead of 2020-12; an explicit `$schema` still wins),
`.forced_spec_version(SpecVersion::Draft07)` (process the whole document, embedded
resources included, as that draft whatever its `$schema` says; documents a `$ref`
loads still follow their own),
`.format_mode(FormatMode::Assertion)` (check `format` whatever the draft: drafts 4,
6 and 7 check it by default, 2019-09 and 2020-12 only record it),
`.integer_semantics(IntegerSemantics::Numeric)` (whether `1.0` is an integer:
draft 4 says no, draft 6 and later yes),
`.exclusive_bounds(ExclusiveBounds::BooleanModifier)` (read `exclusiveMinimum` /
`exclusiveMaximum` as draft 4's boolean modifier or as draft 6's number), and
`.unknown_schema_uri(UnknownSchemaUri::Reject)` (refuse an unrecognized
`$schema` instead of falling back to the default draft with a warning).
`JsonSchema::dialect()` returns a parsed schema's effective rules and
`resolve_dialect(&schema, &settings)` returns them with any warnings. The library
logs warnings at `debug` level through the `log` crate; the CLI prints them to
stderr. CLI: `--jss-default-spec-version` and `--jss-forced-spec-version`
(`draft-04|draft-06|draft-07|2019-09|2020-12`, and `draft-00` to `draft-05`), `--jss-format-mode annotation|assertion`,
`--jss-integer-semantics lexical|numeric`,
`--jss-exclusive-bounds boolean-modifier|numeric`,
`--jss-unknown-schema-uri fallback|reject`.

## Using the macro (compile-time codegen)

The **`json_schema_to_rust!`** macro generates Rust types at compile time and
**inlines** them at the call site (no file is written). Add both crates:

```toml
[dependencies]
json-schema-rs = "0.0.4"
json-schema-rs-macro = "0.0.4"
```

Then `use json_schema_rs_macro::json_schema_to_rust` and use any of these forms:

- **Single file path** (relative to your crate root, i.e. `CARGO_MANIFEST_DIR`):

  `json_schema_to_rust!("path/to/schema.json")`

- **Multiple file paths:** `json_schema_to_rust!("a.json", "b.json")`

- **Single inline JSON Schema string:**
  `json_schema_to_rust!(r#"{"type":"object", "properties": {...}}"#)`

- **Multiple inline JSON Schema strings:**
  `json_schema_to_rust!(r#"..."#, r#"..."#)`

When you pass **multiple** schemas (paths or inline), each schema’s types are
emitted in a **separate Rust module** to avoid name collisions: one module per
JSON Schema. Module names come from the file stem for paths (e.g. `simple` from
`simple.json`) or `schema_0`, `schema_1`, … for inline strings. Use the
generated types via those modules (e.g. `simple::Root`, `schema_0::Root`).

**Reverse codegen (Rust → JSON Schema).** Every generated struct implements
**ToJsonSchema** (e.g. `Root::json_schema()`). Hand-written structs can use
`#[derive(ToJsonSchema)]` from `json_schema_rs_macro` with optional
`#[json_schema(title = "...")]` and, on fields,
**`#[json_schema(minimum = N, maximum = N)]`** (and `exclusive_minimum`,
`exclusive_maximum`, `multiple_of`) to set JSON Schema bounds for
integer/number properties, kept exactly as written; integer types emit their
exact range (`u64` → `0`..`18446744073709551615`). Convert a schema to JSON with
`String::try_from(&schema)` or `.try_into()`. Emits `$defs` and `$ref` for shared
and recursive types, including mutually recursive ones (`A` → `B` → `A`).
Same-name types from different modules get separate entries (`Config0`,
`Config1`); a unique name stays bare. A type with `#[json_schema(id = "...")]`
is its own schema resource; `$ref`s between resources resolve correctly for
absolute and relative `$id`s alike. `json_schema()` returns every problem it
finds (an `$id` that is not a URI reference, two different types whose `$id`s
resolve to one URI) as a `ReverseCodeGenError`. A
hand-written `impl ToJsonSchema` only needs `json_schema()`; implement
`json_schema_with(&mut SchemaGenerator)` to place the type in `$defs`. Add **json-schema-rs-macro** when using the derive. Details:
[design.md](design.md).

Validate a JSON instance against a schema (returns all errors, no fail-fast):

```rust
use json_schema_rs::{validate, JsonSchema};
use serde_json::Value;

let schema: JsonSchema = serde_json::from_str(schema_json)?;
let instance: Value = serde_json::from_str(instance_json)?;
let result = validate(&schema, &instance);
if let Err(errors) = result {
    for err in &errors {
        eprintln!("{}", err);
    }
}
```

`validate` builds the schema's `$ref` registry on every call. To validate many
instances, or to let `$ref` reach other documents, build the registry once:

```rust
use json_schema_rs::{validate_with_registry, JsonSchema, Registry};
use std::collections::HashMap;

let remotes: HashMap<String, Value> = HashMap::from([(
    "https://example.com/address.json".to_string(),
    serde_json::from_str(address_json)?,
)]);
let registry: Registry<'_> = Registry::builder()
    .base_uri("https://example.com/person.json")
    .retriever(remotes)
    .build(&schema)?;
let result = validate_with_registry(&registry, &instance);
```

## Running the binary

Build and run the CLI:

**Build:**

```bash
cargo build --release
```

The binary is at `target/release/jsonschemars`.

**Generate code from one or more JSON Schemas.** The `generate` command takes
one or more INPUTs (file paths, directory paths—recursively searched for `.json`
files—or `-` for one schema from stdin) and writes generated `.rs` files under
the required `-o` output directory. Output file and directory names are
**sanitized** for Rust (e.g. hyphens to underscores), and each directory
includes a `mod.rs` so the output is a valid Rust module tree. If any schema
file fails to read, every failure is logged to stderr with its path and the
command exits without writing output. Only **rust** is supported today. See
[design.md](design.md) for details.

```bash
jsonschemars generate rust -o out/ schema.json
jsonschemars generate rust -o out/ dir1/ dir2/ foo.json
jsonschemars generate rust -o out/ -   # one schema from stdin → out/stdin.rs
```

**Validate a JSON payload against a schema** (one schema, one payload; schema
via `-s`, payload from stdin or `-p`):

```bash
jsonschemars validate -s schema.json < payload.json
```

Both from files: `jsonschemars validate -s schema.json -p payload.json`. Use
`-s -` to read the schema from stdin (payload then from `-p` or stdin).

To generate into a **file at build time** (e.g. under `OUT_DIR`) instead of
using the macro, use the library API from a `build.rs` script:
`let bytes = generate_rust(&[schema], &CodeGenSettings::builder().build())?;`
then write `bytes[0]` to a path and `include!` it in your crate.

## Alternative libraries

TODO

## Developers

**Project is under active maintenance**—even if there are no recent commits.
Please submit an issue or bug request if the library needs updating.

- **Commands:** `make lint`, `make test`, `make fix`
- **Official JSON Schema Test Suite:** `make test` runs every test of the
  [JSON Schema Test Suite](https://github.com/json-schema-org/JSON-Schema-Test-Suite)
  (drafts 4, 6, 7, 2019-09, 2020-12, including `optional/`) as its own test case.
  Run `make vendor_test_suite` once to clone the suite at its pinned commit into
  `research/json-schema-test-suite/` (gitignored); the test hard-fails if it is missing.
  Known failures are listed in
  [`json_schema_rs/tests/suite_xfail.json`](json_schema_rs/tests/suite_xfail.json); the
  run fails on any unexpected failure **and** any unexpected pass. When a fix makes suite
  tests pass, remove their entries (or run `make update_suite_xfail`) and commit the
  manifest. `make test_json_schema_suite` runs only the suite.
- **Known bugs as tests:** every open bug issue has a test in
  `json_schema_rs/tests/known_failures.rs` or `json_schema_rs_macro/tests/known_failures.rs`.
  It asserts the correct behaviour and passes while the bug is present; when the bug is
  fixed it fails, and the fix unwraps it into a normal test. See
  [design.md](design.md#known-failures).
- **Differential testing against other validators:** `make differential` runs our
  validator alongside other JSON Schema implementations (Rust, JS, Python, Java, .NET,
  C++, Go) on the official test suite via [Bowtie](https://github.com/bowtie-json-schema/bowtie)
  and refreshes the committed
  [results](json_schema_rs_differential/results.md). See
  [json_schema_rs_differential/README.md](json_schema_rs_differential/README.md) for
  prerequisites.
- **Implementation and philosophy:** If you're curious how something was
  implemented or the philosophy behind our approach, see [design.md](design.md).

## Credits

Made by [Todd Everett Griffin](https://www.toddgriffin.me/).
