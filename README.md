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
reads `["T", "null"]` as nullable (`Option`, with an absent key distinct from `null`), several non-null types as an untagged enum, and a typeless property as `serde_json::Value`), **boolean
schemas** (`true` / `false` anywhere a schema may appear, including a whole document, which
codegen turns into `serde_json::Value` for `true` and an enum with no variants for `false`), **properties**, **required**, **const** (draft-06+;
validator: instance must equal const, numbers by value so `1` equals `1.0`; codegen: single-value enum (string or any other JSON value); reverse: single-variant unit enum → const), **enum**
(validator compares numbers by value; codegen emits Rust enums for string enums and value enums with exact serde impls for numbers, booleans, null and mixed values), **items** (array with single-schema
items; codegen emits `Vec<T>` or `Option<Vec<T>>`; **uniqueItems**: when true,
codegen emits `Vec<T>` (element order survives a round trip) with `#[json_schema(unique_items)]`, and the validator enforces
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
enforces via regress with Unicode semantics, accepting patterns written for the
grammar without the `u` flag such as `\-` and `\:`; codegen emits `#[json_schema(pattern = "...")]`;
reverse codegen supports the same attribute on String fields), **default** (meta-data only; codegen never fills it in, so an absent key stays absent and a deserialize → serialize round trip gives back the same instance; an optional field gets a `<field>_or_default()` accessor that applies it; codegen and reverse codegen carry it as `#[json_schema(default = ...)]`), **description** (codegen emits Rust doc comments; reverse
codegen from `///` doc comments on structs, enums and fields, or `#[json_schema(description = "...")]`),
**examples** (draft-06+; meta-data only; stored and round-tripped; codegen emits examples in doc comments for structs and enums; not used for validation; Full dedupe includes in key, Functional excludes), **deprecated** (draft 2019-09+; meta-data only; codegen emits `#[deprecated]` on the corresponding field or struct; reverse codegen via `#[json_schema(deprecated = true)]`), **$comment** (draft-07+; stored and round-tripped; not used for validation; reverse codegen via `#[json_schema(comment = "...")]`), and **$schema**
(stored and round-tripped; selects the draft's rules, per schema resource, unless a draft is forced;
reverse codegen emits Draft 2020-12 URI by default; default is Draft 2020-12
when absent; an unrecognized URI falls back to Draft 2020-12 with a warning, or is
rejected with `UnknownSchemaUri::Reject`), and
**$id** (unique identifier; stored and round-tripped; a draft-4 document's `id` is read as `$id`; Full dedupe includes`
$id` in the key, Functional does not; reverse codegen via `#[json_schema(id = \"...\")]`; forward codegen emits the attribute when the schema has `$id`
so round-trip preserves it), **allOf** (validator: instance must validate
against every subschema; codegen: the branches, `$ref`s included, merge into one Rust
type, and an unsatisfiable merge is an enum with no variants; reverse codegen: not supported), **anyOf** (validator: instance must validate against at least one subschema; codegen: `#[serde(untagged)]` enum with one variant per branch (a branch pair with `null` is `Option<T>`; all-string branches are one string enum), so instances deserialize from the bare value; reverse codegen: untagged enums derive `anyOf`), **oneOf** (validator: instance must validate against exactly one subschema; codegen: as anyOf, with `#[json_schema(one_of)]`; exclusivity is not checked on deserialize; reverse codegen: `oneOf` via `#[json_schema(one_of)]`), and **additionalProperties** (validator: false → one error per additional key; schema → validate each additional property value; codegen: unknown keys are kept for a lossless round trip in a `#[serde(flatten)] additional` map (`serde_json::Value`, or the schema's type), false → `#[serde(deny_unknown_fields)]`, overridable with `--cgs-unknown-properties capture|drop`; reverse: closed structs emit false, `BTreeMap<String, V>` emits object with additionalProperties). Recursive schemas (through `#`, `$defs` or mutual
recursion) generate `Box` on by-value cycles only, `if`/`then`/`else` adds the
conditional properties as optional fields, and colliding type names all get a
numeric suffix (`Item0`, `Item1`). For implementation details and design
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
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Address {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub street_address: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<Address>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub age: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub birthday: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}
```

Every generated struct implements **ToJsonSchema** (e.g. `Root::json_schema()`
returns a `Result<JsonSchema, Vec<ReverseCodeGenError>>`). Serialize to JSON with `String::try_from(&schema)` or
`Vec::<u8>::try_from(&schema)`. Reverse codegen emits a flat root-level `$defs`
map with `$ref` for shared and recursive types. See [design.md](design.md) for
reverse codegen details. **`$ref`** resolves through a shared `Registry` for both
validation and Rust codegen: JSON Pointer fragments into any schema location (including
unknown keywords), `$id` / draft-4 `id` resources with RFC 3986 base-URI resolution,
`$anchor` / `$dynamicAnchor`, URNs, the embedded standard
metaschemas, and other documents: other input schemas (by file URI or `$id`),
documents you supply in memory, or a `Retrieve` implementation such as the built-in
`FileRetriever` (files inside a root directory) or `HttpRetriever` (`http`
feature, off by default). The library fetches nothing unless given a retriever. A custom `$schema` metaschema's
`$vocabulary` decides which keyword groups apply. The validator resolves
`$dynamicRef` (2020-12) and `$recursiveRef` (2019-09) against the dynamic scope.

**Validator coverage.** The validator implements every assertion and applicator
keyword of drafts 4, 6, 7, 2019-09 and 2020-12: `not`, `if`/`then`/`else`,
`dependentRequired`, `dependentSchemas`, `dependencies`, `prefixItems`, tuple
`items` with `additionalItems`, `contains` with `minContains`/`maxContains`,
`propertyNames`, `patternProperties`, `minProperties`/`maxProperties`,
`unevaluatedProperties`, `unevaluatedItems`, and every `format` the specifications
define (`date-time`, `date`, `time`, `duration`, `email`, `idn-email`, `hostname`,
`idn-hostname`, `ipv4`, `ipv6`, `uri`, `uri-reference`, `iri`, `iri-reference`,
`uri-template`, `json-pointer`, `relative-json-pointer`, `regex`, `uuid`), asserted
by default in drafts 4/6/7 and annotation-only in 2019-09/2020-12 (override with
`--jss-format-mode`); user-defined formats are registered with
`CompiledSchema::builder().format(name, check)`. Keywords next to `$ref` are evaluated in 2019-09/2020-12 and
ignored in earlier drafts (override with `--jss-ref-siblings`). The official JSON
Schema Test Suite runs as a gate with strict parsing; the cases it still fails
(schemas using a key their draft does not define, numbers beyond 64 bits, draft-7
content assertion) are listed in `json_schema_rs/tests/suite_xfail.json`.

## Using the library

Add to your `Cargo.toml`:

```toml
[dependencies]
json-schema-rs = "0.0.4"
```

Parse one or more schemas and generate Rust (one buffer per schema, plus an
optional shared buffer when dedupe finds identical shapes). A parsed document is a
`Schema`: a schema object or a boolean schema (`true` / `false`). With default
settings you can use **TryFrom**:
`let schema = Schema::try_from(schema_json)?;` or `schema_json.try_into()?`
(from `&str`, `String`, `&[u8]`, `&serde_json::Value`, `std::fs::File`, `&Path` or
`PathBuf`). For custom settings, use the constructors:

```rust
use json_schema_rs::{CodeGenSettings, JsonSchemaSettings, Schema, generate_rust};

let schema_settings = JsonSchemaSettings::default();

// From a string:
let schema = Schema::new_from_str(schema_json_str, &schema_settings)?;
// From bytes (e.g. a buffer or file contents):
let schema = Schema::new_from_slice(&bytes[..], &schema_settings)?;
// From an already-parsed serde_json::Value (avoids string round-trips):
let schema = Schema::new_from_serde_value(&json_value, &schema_settings)?;
// From a reader (e.g. std::fs::File, std::io::Stdin):
let schema = Schema::new_from_reader(reader, &schema_settings)?;
// From a file path:
let schema = Schema::new_from_path("path/to/json-schema.json", &schema_settings)?;

// The schema object, when you need its keywords (`None` for a boolean schema):
let object = schema.as_object();

// Then generate Rust:
let code_gen_settings = CodeGenSettings::builder().build();
let output = generate_rust(&[schema], &code_gen_settings)?;
// output.per_schema.len() == 1; write output.per_schema[0] to a file or stdout
// When dedupe finds shared structs, output.shared is Some(shared_rust_code)
```

A boolean document generates a root like any other schema: `true` accepts every
value (`pub type Root = serde_json::Value;`, as `{}` does) and `false` none (an
enum with no variants).

Structurally identical types are deduplicated by default (one generated
type per shape; types used by several schemas go in a shared buffer that each
per-schema buffer imports as `super::shared::Name`). Use
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
`property-key`, `--cgs-dedupe-mode disabled|functional|full`. Use
`CodeGenSettings::builder().unknown_properties(UnknownProperties::Drop).build()`
(`--cgs-unknown-properties drop`) to ignore unknown object keys instead of keeping
them (faster deserialization), or `Capture` to keep them even where
`additionalProperties` is `false`.

**Cross-document `$ref`.** Give each schema its retrieval URI and a retriever with
`CodeGenInput`; input schemas reach each other by URI or root `$id`, and types a
document shares with another input go in the shared buffer:

```rust
use json_schema_rs::{CodeGenInput, CodeGenSettings, FileRetriever, Schema, file_uri, generate_rust};
use std::path::Path;

let path = Path::new("/schemas/package.json");
let schema: Schema = path.try_into()?;
let input = CodeGenInput::new()
    .schema_at(file_uri(path).unwrap(), &schema)
    .retriever(FileRetriever::new("/schemas")); // `$ref: "ava.json"` reads /schemas/ava.json
let output = generate_rust(input, &CodeGenSettings::builder().build())?;
```

A document that cannot be retrieved is an error naming its URI.

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
`exclusiveMaximum` as draft 4's boolean modifier or as draft 6's number),
`.ref_siblings(RefSiblings::Evaluated)` (whether keywords next to `$ref` count:
drafts 4, 6 and 7 ignore them, 2019-09 and 2020-12 evaluate them), and
`.unknown_schema_uri(UnknownSchemaUri::Reject)` (refuse an unrecognized
`$schema` instead of falling back to the default draft with a warning).
`JsonSchema::dialect()` returns a parsed schema's effective rules and
`resolve_dialect(&schema, &settings)` returns them with any warnings. The library
logs warnings at `debug` level through the `log` crate; the CLI prints them to
stderr. CLI: `--jss-default-spec-version` and `--jss-forced-spec-version`
(`draft-04|draft-06|draft-07|2019-09|2020-12`, and `draft-00` to `draft-05`), `--jss-format-mode annotation|assertion`,
`--jss-integer-semantics lexical|numeric`,
`--jss-exclusive-bounds boolean-modifier|numeric`,
`--jss-ref-siblings ignored|evaluated`,
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

A literal that starts with `{`, or is `true` or `false` (a boolean schema), is
inline JSON; any other literal is a file path.

When you pass **multiple** schemas (paths or inline), each schema’s types are
emitted in a **separate Rust module** to avoid name collisions: one module per
JSON Schema. Module names come from the file stem for paths (e.g. `simple` from
`simple.json`) or `schema_0`, `schema_1`, … for inline strings. Use the
generated types via those modules (e.g. `simple::Root`, `schema_0::Root`).
A `$ref` from a path schema to another file resolves against that schema's
path: the file is read if it is inside your crate root (passing it to the same
macro call also shares its types between the modules). The macro never uses the
network.

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
use json_schema_rs::{validate, Schema, ValidateError};
use serde_json::Value;

let schema: Schema = Schema::try_from(schema_json)?;
let instance: Value = serde_json::from_str(instance_json)?;
match validate(&schema, &instance) {
    Ok(()) => {}
    // The instance does not conform: one error per problem.
    Err(ValidateError::Instance(errors)) => {
        for err in &errors {
            eprintln!("{err}");
        }
    }
    // The schema cannot be used (a `$ref` that does not resolve, an invalid
    // `pattern`, ...): every problem, whatever the instance.
    Err(ValidateError::Schema(problems)) => eprintln!("{problems}"),
}
```

`validate` builds the schema's `$ref` registry and compiles the schema on every call.
A schema that cannot be used is `ValidateError::Schema`, never an instance error:
`SchemaError::Registry` when the registry cannot be built, `SchemaError::Compile`
when the schema does not compile, each listing every problem.
To validate many instances, or to let `$ref` reach other documents, build the
registry and compile the schema once; `is_valid` is the fastest check when you only
need the verdict:

```rust
use json_schema_rs::{CompiledSchema, Registry};
use std::collections::HashMap;

let remotes: HashMap<String, Value> = HashMap::from([(
    "https://example.com/address.json".to_string(),
    serde_json::from_str(address_json)?,
)]);
let registry: Registry<'_> = Registry::builder()
    .base_uri("https://example.com/person.json")
    .retriever(remotes)
    .build(&schema)?;
let compiled: CompiledSchema = CompiledSchema::new(&registry)?;
let result = compiled.validate(&instance);
let valid: bool = compiled.is_valid(&instance);
```

Each `ValidationError` carries the instance location, the failing keyword's location
through the schema as evaluated (`keyword_location`, e.g. `/properties/width/$ref/minimum`),
its absolute URI after the last `$ref` crossed (`absolute_keyword_location`, when the
schema has one), and the `kind` of problem. A failing `anyOf` / `oneOf` lists every
branch's errors as its `causes`. The standard output formats of JSON Schema 2020-12
(§12) are rendered with `compiled.evaluate(&instance, OutputFormat::Basic)` (`Flag`,
`Basic` or `Detailed`), which serializes to the specification's JSON:

```rust
use json_schema_rs::{Output, OutputFormat};

let output: Output = compiled.evaluate(&instance, OutputFormat::Detailed);
println!("{}", serde_json::to_string_pretty(&output)?);
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

A `$ref` to another file resolves against the referring file's path (or its
`$id`). Files passed as INPUTs reach each other by path or `$id`; any other file
is read only from inside `--cgs-file-root DIR` (default: the current directory).
`http:` / `https:` documents are fetched only with `--cgs-allow-remote-refs`,
which needs the binary built with the `http` feature
(`cargo install json-schema-rs --features http`).

**Validate a JSON payload against a schema** (one schema, one payload; schema
via `-s`, payload from stdin or `-p`):

```bash
jsonschemars validate -s schema.json < payload.json
```

Both from files: `jsonschemars validate -s schema.json -p payload.json`. Use
`-s -` to read the schema from stdin (payload then from `-p` or stdin). Exit
codes: `0` the payload conforms; `1` it does not (one line per error on stderr);
`2` no verdict, because the schema or payload could not be read or parsed, or the
schema cannot be used (one `schema error:` line per problem).

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
