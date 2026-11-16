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
reads `["T", "null"]` as nullable (`Option<T>` when required; `json_schema_rs::Nullable<T>`, which keeps an absent key distinct from `null`, when optional), several non-null types as an untagged enum, and a typeless property as `serde_json::Value`), **boolean
schemas** (`true` / `false` anywhere a schema may appear, including a whole document, which
codegen turns into `serde_json::Value` for `true` and an enum with no variants for `false`), **properties**, **required**, **const** (draft-06+;
validator: instance must equal const, numbers by value so `1` equals `1.0`; codegen: single-value enum (string or any other JSON value); reverse: single-variant unit enum → const), **enum**
(validator compares numbers by value; codegen emits Rust enums for string enums and value enums with exact serde impls for numbers, booleans, null and mixed values), **items** (array with single-schema
items; codegen emits `Vec<T>` or `Option<Vec<T>>`; **uniqueItems**: when true,
codegen emits `Vec<T>` by default (element order survives a round trip) with `#[json_schema(unique_items)]`, or a
`HashSet`, `BTreeSet` or `IndexSet` (`indexmap` feature) with `--cgs-unique-items` / `UniqueItemsType`, which rejects a duplicate item when deserializing; the validator enforces
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
reverse codegen supports the same attribute on String fields), **default** (meta-data only; codegen never fills it in, so an absent key stays absent and a deserialize → serialize round trip gives back the same instance; an optional field gets a `<field>_or_default()` accessor that applies it; codegen and reverse codegen carry it as `#[json_schema(default = ...)]`), **description** (codegen emits Rust doc comments, rendered so they pass clippy's and rustdoc's documentation lints; reverse
codegen from `///` doc comments on structs, enums and fields, or `#[json_schema(description = "...")]` on any of them),
**examples** (draft-06+; meta-data only; stored and round-tripped; codegen emits examples in doc comments for structs and enums; not used for validation; Full dedupe includes in key, Functional excludes), **deprecated** (draft 2019-09+; meta-data only; codegen emits `#[deprecated]` on the corresponding field or struct; reverse codegen via `#[json_schema(deprecated = true)]`), **$comment** (draft-07+; stored and round-tripped; not used for validation; reverse codegen via `#[json_schema(comment = "...")]`), and **$schema**
(stored and round-tripped; selects the draft's rules, per schema resource, unless a draft is forced;
reverse codegen emits Draft 2020-12 URI by default; default is Draft 2020-12
when absent; an unrecognized URI falls back to Draft 2020-12 with a warning, or is
rejected with `UnknownSchemaUri::Reject`), and
**$id** (unique identifier; stored and round-tripped; a draft-4 document's `id` is read as `$id`; Full dedupe includes`
$id` in the key, Functional does not; reverse codegen via `#[json_schema(id = \"...\")]`; forward codegen emits the attribute when the schema has `$id`
so round-trip preserves it), **allOf** (validator: instance must validate
against every subschema; codegen: the branches, `$ref`s included, merge into one Rust
type, and an unsatisfiable merge is an enum with no variants; reverse codegen: not supported), **anyOf** (validator: instance must validate against at least one subschema; codegen: `#[serde(untagged)]` enum with one variant per branch (a branch pair with `null` is `Option<T>`), so instances deserialize from the bare value, into a variant whose branch the value satisfies (self-validating types, the default; off, the first variant that fits); reverse codegen: untagged enums derive `anyOf`, tagged enums `oneOf`), **oneOf** (validator: instance must validate against exactly one subschema; codegen: as anyOf, with `#[json_schema(one_of)]`; deserializing checks exclusivity with self-validating types, the default; reverse codegen: `oneOf` via `#[json_schema(one_of)]`; a one-entry `anyOf` / `oneOf` can be generated as its entry's type, see *Using the library*), and **additionalProperties** (validator: false → one error per additional key; schema → validate each additional property value; codegen: unknown keys are kept for a lossless round trip in a `#[serde(flatten)] additional` map (`serde_json::Value`, or the schema's type), false → `#[serde(deny_unknown_fields)]`, overridable with `--cgs-unknown-properties capture|drop`; beside `patternProperties` the map's values take the matching pattern's type, or an untagged enum of every pattern's and `additionalProperties`' types; reverse: closed structs emit false, `BTreeMap<String, V>` emits object with additionalProperties). Recursive schemas (through `#`, `$defs` or mutual
recursion) generate `Box` on by-value cycles, a field or union variant whose struct or union
type would be large by value (over 256 bytes) holds it in a `Box` (so no generated value is large
on the stack), `if`/`then`/`else` adds the
conditional properties as optional fields (typed only where they hold unconditionally: a
generated type accepts every instance its schema accepts), and colliding type names all get a
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

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

#[derive(Debug, Clone, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

Generated structs derive `Deserialize` from `json-schema-rs-macro`, not serde: it
reads each field with one call to a shared generic function, so an optimized build of a
struct with hundreds of fields (package.json's ESLint `Rules` has 864) takes seconds
where serde's derive takes minutes. It reads a JSON object with serde's derive semantics
(missing, repeated and unknown keys, with serde's messages) and reads the
`#[serde(flatten)]` map of unknown keys key by key, without buffering the input.

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
`--jss-format-mode`); an unknown format is ignored unless `--jss-unknown-formats reject`
makes it a schema error wherever formats are asserted. User-defined formats are
registered with `CompiledSchema::builder().format(name, |s: &str| ...)` (strings) or
`.value_format(name, |v: &Value| ...)` (any instance type, e.g. `int32`). Custom keywords are
defined with `CompiledSchema::builder().keyword(name, |value: &Value| ...)`, which builds a
`CustomKeyword` from each occurrence's value at compile time; a custom metaschema's vocabulary
for them is declared with `Registry::builder().vocabulary(uri)`. `contentEncoding` (`base64`) and
`contentMediaType` (`application/json` and any `+json` type) are asserted by default in
draft 7, which allows it, and annotation-only in 2019-09/2020-12, which must not decode
content by default (override with `--jss-content-mode`); other encodings and media types,
and `contentSchema`, are annotations. Encodings and media types are added with
`CompiledSchema::builder().content_encoding(name, |s: &str| ...)` (the decoded bytes, or
`None`) and `.content_media_type(name, |content: &[u8]| ...)`, and built-in ones removed
with `.without_content_encoding(name)` / `.without_content_media_type(name)`. For schemas from
untrusted sources, `.regex_engine(RegexEngine::Linear)` (CLI `--regex-engine linear`; the
`linear-regex` feature, off by default) matches `pattern` and `patternProperties` in linear
time, refusing backreferences and lookaround, `.regex_size_limit(Some(bytes))` caps each
pattern's memory (same feature), and `.max_depth(Some(n))` caps
evaluation depth (CLI `--regex-size-limit`, `--max-depth`). `.max_errors(Some(n))` ends
`validate` at its `n`-th error (CLI `--max-errors`), `compiled.iter_errors(&instance)` finds
errors as they are taken (`.next()` costs about one error), and `error.masked()` displays an
error without the instance's values, for logs of sensitive data (CLI `--masked`).
Instances need not be `serde_json::Value`: every check (`is_valid`, `validate`,
`iter_errors`, `evaluate`, `annotations`) takes any type that implements the `Instance`
trait, so a document already held in another JSON tree (a `simd-json` value, a YAML or TOML
tree, a language binding's objects) is validated without converting it first.
`compiled.validate_with_defaults(&mut instance)` inserts the `default` of every missing
property, then validates (CLI `--apply-defaults`); `validate` never changes the instance. `registry.bundle()` writes the schema and every document its
references reach as one document, each embedded as a schema resource (2020-12 core §9.3;
CLI `jsonschemars bundle`); `registry.dereference()` replaces every reference by its target,
for tools that do not resolve references (a recursive reference becomes a JSON Pointer to
its ancestor; CLI `jsonschemars dereference`); `registry.canonicalize()` rewrites the schema
into one normal form that keeps its verdicts, annotations and references, so equivalent
schemas compare equal more often (CLI `jsonschemars canonicalize`). Keywords next to `$ref` are evaluated in 2019-09/2020-12 and
ignored in earlier drafts (override with `--jss-ref-siblings`). The official JSON
Schema Test Suite runs as a gate with the default settings. It passes every test except draft 4's
bignum integers in the default build (numbers beyond 64 bits, listed in `json_schema_rs/tests/suite_xfail.json`),
which the `arbitrary-precision` feature passes.

**Numbers** are compared exactly, never with an epsilon. By default every integer
within 64 bits (`u64` / `i64`) is exact; beyond that `serde_json` reads a number as the
nearest `f64`, so `18446744073709551616` and `18446744073709551617` compare equal, and a
number beyond `f64` range (`1e400`) does not parse. The opt-in `arbitrary-precision`
feature makes every number exact as written (bounds, `multipleOf`, `const`, `enum`,
`uniqueItems`, draft 4's lexical `integer`):

```toml
json-schema-rs = { version = "0.0.4", features = ["arbitrary-precision"] }
```

It enables `serde_json/arbitrary_precision`, which Cargo turns on for every crate in
your build; with it, serde's derived `#[serde(flatten)]` and `#[serde(untagged)]`
cannot deserialize numbers in your own types (generated code avoids both, so it works
in either build). A generated type embeds its schema text, so a schema holding a
number beyond `f64` needs the feature where the generated code runs too.

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
per-schema buffer imports as `super::shared::Name`). Self-validating types
merge when their schemas validate the same, wherever they sit: two documents
that define the same `Address` share one type. Use
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
`additionalProperties` is `false`. `uniqueItems` arrays are `Vec<T>` by default, the
spec's ordered array, kept exactly; `.unique_items(UniqueItemsType::HashSet)`
(`--cgs-unique-items hash-set`), `BTreeSet` (`btree-set`, sorted output) or `IndexSet`
(`index-set`, order kept; needs the `indexmap` feature here, and `indexmap` with `serde`
plus `json-schema-rs` with `indexmap` in the crate that compiles the generated code)
make uniqueness part of the type: a duplicate item is a deserialize error, never dropped.
A set applies to a property whose value is the array when Rust equality is JSON
equality for the item type: strings, integers, booleans, generated enums, and generated
structs and unions built from these (which then derive `Hash`, `Eq` and `Ord`; a struct
needs `additionalProperties: false` or a typed `additionalProperties`). Anything else
(floats, any value, `Uuid`, a struct with a set field) stays `Vec`. Maps (an object with
no properties, and a struct's flattened unknown keys) are `BTreeMap<String, T>` by
default, with sorted, machine-independent output; `.map_type(MapType::HashMap)`
(`--cgs-map-type hash-map`) or `IndexMap` (`index-map`, instance key order kept; the same
`indexmap` requirements as `IndexSet`) picks another map. A struct holding a `HashMap` or
`IndexMap` has no `Hash` / `Ord`, so it stays out of `uniqueItems` sets. `.flatten_single_entry_combinators(true)`
(`--cgs-flatten-single-entry-combinators`) generates an `anyOf` / `oneOf` with exactly
one entry as that entry's type instead of a one-variant enum (off by default, so the
derive gives the combinator back); `#[json_schema(flatten_single_entry)]` on a
one-variant untagged enum derives its payload's schema instead of a one-entry `anyOf`.

**Builders.** `.builders(true)` (`--cgs-builders`) adds a `builder` submodule to each
generated file with a builder per struct: `Veg::builder().name("carrot").weight(3_u8).try_into()?`.
Each setter takes anything `TryInto` the field's type (an optional field's setter takes the
inner type), and `try_into()` (`TryFrom<builder::Veg>`) reports every missing required field
and failed conversion together in a `json_schema_rs::BuilderError`; a self-validating struct
is validated as well. `builder::Veg::from(veg)` starts from an existing value.

**Enum and default impls.** String enums implement `as_str()`, `Display` and `FromStr`
with their JSON values (`"sweet potato".parse::<Veg>()`); `.string_enum_impls(false)`
(`--cgs-string-enum-impls false`) leaves them out. `.schema_defaults(true)`
(`--cgs-schema-defaults`) makes a type with a schema `default` (or a struct whose properties
have defaults) implement `Default` with it, when the default is valid for both the schema
and the Rust type; otherwise `default` stays an annotation, read through the
`<field>_or_default()` accessors.

**Fitting the output into your crate.** `.root_name("Config")` (`--cgs-root-name Config`)
names each schema's root type, as written, ahead of its title. `.visibility(Visibility::Crate)`
(`--cgs-visibility crate`) makes every generated item and field `pub(crate)`.
`.shared_path("crate::common")` (`--cgs-shared-path`) is where each file imports deduplicated
types from (default `super::shared`), and `.header("Copyright ...")` (`--cgs-header`) opens
every generated file with `//` comment lines.

**Integers.** From draft 6 on an integer is any number with a zero fraction, so generated
integer types accept `1.0` and `2e0` (fields, items, map values, union variants and
newtypes read through `json_schema_rs::integer`); draft 4 accepts only numbers written as
integers. A fraction, or a number the Rust type cannot hold, fails to deserialize. An
integer without both bounds is `i64`: a valid integer beyond its range (`2^63`) fails to
deserialize; give the schema `minimum` / `maximum` for a wider type (`u64`). With
self-validating types off, a root map or array is a type alias of a std type
(`pub type Root = Vec<i64>;`) that deserializes through serde's own impl; read it through
`json_schema_rs::integer::Integer<Root>` to apply the rule.

**Typed formats (a deliberate divergence).** Codegen gives a format with a Rust type that
type in every draft: `uuid` → `uuid::Uuid` (the `uuid` feature), `date-time` →
`chrono::DateTime<chrono::FixedOffset>` and `date` → `chrono::NaiveDate` (the `chrono`
feature), `ipv4` / `ipv6` → `std::net::Ipv4Addr` / `Ipv6Addr`. A string that is not one fails
to deserialize. In 2019-09 and 2020-12 `format` is only an annotation by default, so the
schema itself accepts any string there; we diverge on purpose, since a schema author who
writes `uuid` means a UUID. `.format_types(FormatTypes::FollowDialect)`
(`--cgs-format-types follow-dialect`) is the spec-exact choice: the typed field only where
the dialect asserts formats (`format_mode` `Assertion`), else `String`. Where the dialect
asserts, the field is also checked against the format's exact grammar, which the parsers
are looser than (`Uuid` accepts `{...}`, `chrono` an unpadded `2024-1-5`). `time` and `uri`
stay `String`: no crate's type accepts exactly what the format does. The validator follows
the spec either way.

**Self-validating types (on by default).** Generated types enforce their whole schema
through this library's validator, with the same errors (`ValidationError`s) it gives:
every generated struct, enum and union gets `validate(&self)` and
`TryFrom<serde_json::Value>` (validate, then deserialize), from one
`json_schema_rs::self_validating!` line per type (a root map or array is a newtype,
`pub struct Root(pub Vec<String>);`, so it gets them too), and an `anyOf` / `oneOf` union
deserializes into a variant whose branch the value satisfies (`oneOf`: exactly one branch,
so a value two branches accept is an error), not the first variant serde can fill.
Constraints the Rust type cannot express (`minLength`, `pattern`, `maximum`,
`multipleOf`, `not`, `if` / `then` / `else`, ...) are all checked. The generated code
embeds only the parts of the schema documents its types' validation reads (annotations
and unused `$defs` are pruned, and the pruned copy is proven to validate identically)
and compiles them once, on first use; it then depends on `json-schema-rs` and
`serde_json` at run time. The output is the same on every machine: files are identified
by their path relative to the inputs' common directory, never by an absolute path.
Codegen compiles the schema first, so one the validator cannot use (an invalid `pattern`)
is a generation error. `.self_validating(false)` (`--cgs-self-validating false`) generates
plain data types: only the Rust shape is checked, and a union takes the first variant
that fits.

A struct's plain `Deserialize` (`serde_json::from_str::<Person>`) is **not** a full
check: it checks the Rust shape, constrained newtypes and union variants, but not array,
object or applicator keywords (`minItems`, `uniqueItems`, `maxProperties`, `not`, ...).
Deserialize `json_schema_rs::Validated<Person>` instead to check the whole schema
wherever a framework deserializes for you (`axum::Json<Validated<Person>>`,
`actix_web::web::Json<Validated<Person>>`, `response.json::<Validated<Person>>()`): it
reads the value through `TryFrom`, so an invalid instance fails with every error.

```rust
let person: Person = Person::try_from(serde_json::json!({"name": "A"}))?; // Err: name too short
person.validate()?;
let person: Validated<Person> = serde_json::from_str(r#"{"name": "A"}"#)?; // Err: name too short
```

**Constrained newtypes (on by default).** Codegen generates a newtype for each string, integer, number or
boolean whose schema asserts more than its Rust type holds (`minLength`, `maxLength`,
`pattern`, a checked `format`, bounds narrower than the integer type, any bound on a
float, `multipleOf`, `not`, ...): `pub struct Name(String);` with a private field, so it
only ever holds a valid value. `TryFrom` (and `FromStr` / `TryFrom<&str>` for strings)
and `Deserialize` validate through the same embedded validator, with its errors, so an
invalid value is rejected at the field while deserializing; `Deref`, `AsRef`,
`into_inner`, `From<Name>` and `Display` read it, and `Serialize` is the scalar's own (the
impls come from one `json_schema_rs::constrained_newtype!` line per type). A
`$defs` entry is named after its key, and the derive gives the constraints back. Errors
while deserializing stop at the first invalid field (serde's behaviour); `TryFrom<serde_json::Value>`
on a self-validating type reports every error. `.constrained_newtypes(false)`
(`--cgs-constrained-newtypes false`) keeps such scalars as plain `String` / `u8` / `f64`;
the setting is independent of self-validating types.

```rust
let name: Name = "Ada".parse()?; // Err for "" (minLength 1)
let person: Person = serde_json::from_str(r#"{"name": ""}"#)?; // Err at the field
```

**Extra derives, attributes and patches.** `.extra_derives(["schemars::JsonSchema"])`
(`--cgs-derive`) and `.extra_attrs(["non_exhaustive"])` (`--cgs-attr`, the text inside
`#[...]`) add to every generated struct, enum and union; a derive the type already has
(compared by its last path segment, `Serialize` and `Deserialize` included) is not
repeated, and a plain `pub type` alias takes none. `.patch("Veg", TypePatch { rename:
Some("Vegetable".into()), derives: vec!["Hash".into()], attrs: vec![] })` (`--cgs-patch
FILE`, a JSON object `{"Veg": {"rename": "Vegetable", "derives": ["Hash"]}}`) renames one
generated type everywhere it is used and adds derives and attributes to it alone; patches
are keyed by the generated name. A patch for a name nothing has, a new name that is not a
Rust identifier or that another type in its module has, and a malformed derive or
attribute are errors, all reported at once.

**Your own types.** `.convert(json!({"type": "string", "format": "decimal"}),
"rust_decimal::Decimal")` (`--cgs-convert 'rust_decimal::Decimal={"type":"string","format":"decimal"}'`)
uses your type wherever a subschema equals that schema, ignoring annotations (`title`,
`description`, ...); a nullable copy (`"type": ["string", "null"]`) becomes an `Option`,
and the first matching conversion wins. `.replace("Ipv4", "my::Ipv4")` (`--cgs-replace
Ipv4=my::Ipv4`) uses your type in place of the generated type of that name, which is no
longer generated (nor are the types only it used). A replacement type implements
`Debug`, `Clone`, `PartialEq`, `Serialize`, `Deserialize` and `ToJsonSchema`; a converted
type needs no `ToJsonSchema`, since the generated code keeps the subschema for it.
Self-validating types validate the JSON before converting it (`TryFrom<Value>`,
`Validated<T>`, and union variants while deserializing); a plain `Deserialize` takes
what your type takes.

**Types of other crates (`x-rust-type`).** A subschema carrying
`"x-rust-type": {"crate": "my-types", "version": "^1.2", "path": "my_types::Thing"}` (with
optional `"parameters": [schema, ..]` for a generic type) is ignored by default, as JSON
Schema ignores unknown keywords. `.external_crate("my-types", CrateSpec { version:
"1.2".parse()?, rename: None })`
(`--cgs-crate my-types@1.2`, `[rename=]crate@version`, `*` for any version, `!` for none)
uses `::my_types::Thing` wherever the crate's version satisfies the requirement, and
generates the subschema otherwise; `.unknown_crates(UnknownCrates::Allow)`
(`--cgs-unknown-crates allow`) uses the types of unlisted crates too, and `Deny` makes them
an error. The named types implement `Serialize`, `Deserialize` and `ToJsonSchema`, as types
generated from the same schema do. From typify: `cargo typify s.json --crate my-types@1.2
--unknown-crates deny` is `jsonschemars generate rust -o out/ s.json --cgs-crate
my-types@1.2 --cgs-unknown-crates deny`.

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
`.content_mode(ContentMode::Assertion)` (check `contentEncoding` and
`contentMediaType` whatever the draft: draft 7 checks them by default, 2019-09 and
2020-12 only record them),
`.unknown_formats(UnknownFormats::Reject)` (refuse a schema that asserts a format
neither the draft nor a user format defines, instead of ignoring it),
`.integer_semantics(IntegerSemantics::Numeric)` (whether `1.0` is an integer:
draft 4 says no, draft 6 and later yes),
`.exclusive_bounds(ExclusiveBounds::BooleanModifier)` (read `exclusiveMinimum` /
`exclusiveMaximum` as draft 4's boolean modifier or as draft 6's number),
`.ref_siblings(RefSiblings::Evaluated)` (whether keywords next to `$ref` count:
drafts 4, 6 and 7 ignore them, 2019-09 and 2020-12 evaluate them), and
`.unknown_schema_uri(UnknownSchemaUri::Reject)` (refuse an unrecognized
`$schema` instead of falling back to the default draft with a warning), and
`.metaschema_check(MetaschemaCheck::Validate)` (check the schema against its
draft's metaschema, drafts 4 to 2020-12, and refuse it with every failure; off by
default). `json_schema_rs::meta::validate(&schema_value)` and `meta::is_valid` run
the same check on a `serde_json::Value` (the `jsonschema` crate's `meta::validate`).
`JsonSchema::dialect()` returns a parsed schema's effective rules and
`resolve_dialect(&schema, &settings)` returns them with any warnings. The library
logs warnings at `debug` level through the `log` crate; the CLI prints them to
stderr. CLI: `--jss-default-spec-version` and `--jss-forced-spec-version`
(`draft-04|draft-06|draft-07|2019-09|2020-12`, and `draft-00` to `draft-05`), `--jss-format-mode annotation|assertion`,
`--jss-content-mode annotation|assertion`,
`--jss-unknown-formats ignore|reject`,
`--jss-integer-semantics lexical|numeric`,
`--jss-exclusive-bounds boolean-modifier|numeric`,
`--jss-ref-siblings ignored|evaluated`,
`--jss-unknown-schema-uri fallback|reject`,
`--jss-metaschema-check skip|validate`.

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

**Settings** follow the schema literals as `name = value`: every
`CodeGenSettings` and `JsonSchemaSettings` field, named like its builder method.
A flag takes `true` or `false`, an enum its variant name, and a setting left out
keeps the builder's default:

```rust
json_schema_to_rust!(
    "schemas/a.json",
    dedupe_mode = Disabled,
    unique_items = BTreeSet,
    flatten_single_entry_combinators = true,
    forced_spec_version = Draft07,
);
```

- Codegen: `model_name_source = TitleFirst|PropertyKeyFirst`,
  `dedupe_mode = Disabled|Functional|Full`, `unknown_properties = Capture|Drop`,
  `unique_items = Vec|HashSet|BTreeSet|IndexSet` (`IndexSet` needs the macro's
  `indexmap` feature), `map_type = BTreeMap|HashMap|IndexMap` (`IndexMap` needs the
  same feature), `flatten_single_entry_combinators`, `self_validating`,
  `constrained_newtypes`, `format_types = Typed|FollowDialect`,
  `extra_derives = [schemars::JsonSchema]`, `extra_attrs = [non_exhaustive]`,
  `patch = { Veg = { rename = "Vegetable", derives = [Hash], attrs = [..] } }`,
  `convert = { {"type":"string","format":"decimal"} => rust_decimal::Decimal }`,
  `replace = { Ipv4 = my::Ipv4 }`, `crates = ["my-types@1.2"]`,
  `unknown_crates = Generate|Allow|Deny`, `builders`, `string_enum_impls`,
  `schema_defaults`, `root_name = "Config"`, `visibility = Public|Crate`. (The macro places the
  shared module itself and its expansion carries no comments, so `shared_path` and `header`
  are library and CLI settings.)
- Schema ingestion: `disallow_unknown_fields`,
  `forced_spec_version` / `default_spec_version = Draft04|Draft06|Draft07|Draft201909|Draft202012`
  (and `Draft00` to `Draft05`), `format_mode = Annotation|Assertion`,
  `content_mode = Annotation|Assertion`, `unknown_formats = Ignore|Reject`, `integer_semantics = Lexical|Numeric`,
  `exclusive_bounds = BooleanModifier|Numeric`, `ref_siblings = Ignored|Evaluated`,
  `unknown_schema_uri = Fallback|Reject`, `metaschema_check = Skip|Validate`. They apply to the input schemas and to
  every file a `$ref` reads.

An unknown setting, an unknown value, a setting given twice and
`unique_items = IndexSet` or `map_type = IndexMap` without the `indexmap` feature are compile errors at
the offending token, all reported at once.

Editing a schema file the macro read (a path input, or a file one of its `$ref`s
names) rebuilds your crate on the next `cargo build`.

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
`json_schema_with(&mut SchemaGenerator)` to place the type in `$defs`. Add **json-schema-rs-macro** when using the derive.

The derived schema accepts exactly what the type's serde `Deserialize` accepts, so
a type moves from schemars by changing only the derive name. It follows serde's
`rename`, `rename_all`, `rename_all_fields`, `alias`, `default`, `skip`, `skip_deserializing`,
`deserialize_with`, `flatten` (structs, maps, enums, `Option`s of them),
`transparent`, `from` / `try_from`, every enum representation (external,
`tag`, `tag` + `content`, `untagged`, a variant's `untagged`, `other`), unit and
tuple structs. On a field, `#[json_schema(with = "Type")]` takes another type's
schema and `#[json_schema(schema_with = "path")]` a function's. Details:
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

With the `async` feature, `RegistryBuilder::async_retriever` takes an `AsyncRetrieve`
implementation and `build_async` fetches the documents each round of discovery finds
concurrently (the counterpart of jsonschema's `async_options().with_retriever(..)` and
ajv's `loadSchema`):

```rust
use json_schema_rs::{AsyncRetrieve, Registry, RetrieveError, RetrieveFuture};

struct MyAsync;

impl AsyncRetrieve for MyAsync {
    fn retrieve<'a>(&'a self, uri: &'a str) -> RetrieveFuture<'a> {
        Box::pin(async move { fetch_json(uri).await.map_err(|e| RetrieveError { reason: e.to_string() }) })
    }
}

let registry: Registry<'_> = Registry::builder()
    .async_retriever(MyAsync)
    .build_async(&schema)
    .await?;
```

Each `ValidationError` carries the instance location, the failing keyword's location
through the schema as evaluated (`keyword_location`, e.g. `/properties/width/$ref/minimum`),
its absolute URI after the last `$ref` crossed (`absolute_keyword_location`, when the
schema has one), and the `kind` of problem. A failing `anyOf` / `oneOf` lists every
branch's errors as its `causes`. The standard output formats of JSON Schema 2020-12
(§12) are rendered with `compiled.evaluate(&instance, OutputFormat::Basic)` (`Flag`,
`Basic`, `Detailed` or `Verbose`, which reports every subschema evaluated), which
serializes to the specification's JSON:

```rust
use json_schema_rs::{Output, OutputFormat};

let output: Output = compiled.evaluate(&instance, OutputFormat::Detailed);
println!("{}", serde_json::to_string_pretty(&output)?);
```

**Annotations** (`title`, `description`, `default`, `examples`, `readOnly`,
`writeOnly`, `deprecated`, `format`, the content keywords and unknown keywords such
as `x-widget`) are collected when the schema is compiled with `annotations(true)`.
`compiled.annotations(&instance)` lists the ones a valid instance collects, each with
its keyword, value, instance location and schema locations (those of failing
subschemas are dropped, as the specification requires), and `evaluate` includes them
in `Basic`, `Detailed` and `Verbose` output. The setting is off by default and costs
`validate` and `is_valid` nothing.

```rust
use json_schema_rs::{Annotation, CompiledSchema};

let compiled: CompiledSchema = CompiledSchema::builder().annotations(true).build(&registry)?;
let annotations: Vec<Annotation> = compiled.annotations(&instance).expect("valid instance");
```

## Running the binary

The CLI is its own crate, `jsonschemars`, so the library's users never build its
dependencies:

```bash
cargo install jsonschemars
```

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
is read only from inside `--file-root DIR` (default: the current directory).
`http:` / `https:` documents are fetched only with `--allow-remote-refs`,
which needs the binary built with the `http` feature
(`cargo install jsonschemars --features http`).

**Validate a JSON payload against a schema** (schema via `-s`, payload from
stdin or `-p`):

```bash
jsonschemars validate -s schema.json < payload.json
```

Both from files: `jsonschemars validate -s schema.json -p payload.json`. Use
`-s -` to read the schema from stdin (payload then from `-p` or stdin).

`$ref`s to other documents resolve like `generate`'s: against the schema file's
path (or its `$id`), reading other files only from inside `--file-root DIR`
(default: the current directory) and `http:` / `https:` documents only with
`--allow-remote-refs`. Pass documents the schema references with
`-r/--resource FILE` (repeatable); each is reached by its path or its root `$id`,
so a schema that refers to `https://` URLs validates offline with those documents
vendored:

```bash
jsonschemars validate -s package.json -r ava.json -r eslintrc.json -p my-package.json
```

For a schema from an untrusted source, `--regex-engine linear` matches patterns in
linear time (a pattern with a backreference or lookaround is then a schema error),
`--regex-size-limit BYTES` caps the memory each pattern compiles to, and
`--max-depth LEVELS` caps how deeply evaluation nests. The first two need the binary built
with the `linear-regex` feature (`cargo install jsonschemars --features linear-regex`):

```bash
jsonschemars validate -s untrusted.json --regex-engine linear --regex-size-limit 10000000 --max-depth 64 -p payload.json
```

`--max-errors COUNT` reports at most that many errors and ends evaluation at the last
one, and `--masked` prints each error without the payload's values
(`/age: value is above maximum 10`), for logs of sensitive data:

```bash
jsonschemars validate -s schema.json --max-errors 1 --masked -p payload.json
```

`--apply-defaults` inserts the `default` of every missing property the schema lists
before validating, and prints the payload with its defaults to stdout when it conforms:

```bash
jsonschemars validate -s config.schema.json --apply-defaults -p config.json > config.filled.json
```

`--output FORMAT` prints the result to stdout as JSON in a standard output format:
`flag`, `basic`, `detailed` or `verbose` (every format but `flag` includes the
annotations of a valid payload):

```bash
jsonschemars validate -s schema.json -p payload.json --output basic
```

`-p` is repeatable: the schema compiles once and validates every payload. Each error line
then starts with the payload's path, and `--output` prints a JSON array of
`{"instance": PATH, "output": ...}` records in the order given; `--errors-only` leaves the
conforming payloads out of it:

```bash
jsonschemars validate -s schema.json -p a.json -p b.json --output basic --errors-only
```

Without `-s`, each payload's `$schema` names its schema, resolved against the payload's
path (a `$ref` like any other; the standard metaschemas are built in, so a schema file
validates against its own `$schema` offline):

```bash
jsonschemars validate -p config.json -p my-schema.json
```

Files ending in `.yaml` or `.yml` (schema, resources and payloads) are read as YAML.

For `--allow-remote-refs`, `--connect-timeout SECONDS` and `--timeout SECONDS` (default 30)
bound each request, `--cacert FILE` adds the PEM certificates in FILE to the trusted roots,
and `-k/--insecure` accepts any certificate:

```bash
jsonschemars validate -s https-refs.json --allow-remote-refs --timeout 5 --cacert ca.pem -p payload.json
```

Exit codes: `0` every payload conforms; `1` a payload does not (one line per error on
stderr, or the `--output` JSON on stdout); `2` no verdict for a payload, because a schema
document or the payload could not be read or parsed, or the schema cannot be used (one
`schema error:` line per problem).

Coming from another validator's CLI: `jsonschema-cli validate schema.json -i a.json -i b.json`
and `boon schema.json a.json b.json` become `jsonschemars validate -s schema.json -p a.json
-p b.json`; jsonschema-cli's `--output list` and boon's `--output basic` are
`--output basic`, and `--output hierarchical` / boon's `--output detailed` are
`--output detailed`; `--errors-only`, `--connect-timeout`, `--timeout`, `-k/--insecure`
and `--cacert` keep their names.

`jsonschemars bundle SCHEMA` (or `-s SCHEMA`) writes one schema document holding every
document the schema references, so it needs no other files (`--resource`, `--file-root` and
`--allow-remote-refs` as for `validate`; exit `0` written, `1` the output could not be
written, `2` a document could not be read or the schema cannot be used):

```bash
jsonschemars bundle order.json -r vendored-customer.json -o order.bundle.json
```

`jsonschemars dereference SCHEMA` takes the same arguments and writes the schema with every
reference replaced by its target (exit `2` also when the schema reaches a document of
another draft, which cannot be inlined):

```bash
jsonschemars dereference order.json -o order.dereferenced.json
```

`jsonschemars canonicalize SCHEMA` takes the same arguments and writes the schema in one
normal form: keywords sorted; `type`, `enum` and `required` deduplicated and sorted; a
one-value `enum` as `const`; assertions that accept everything and bounds another bound
implies removed; `{}` as `true`. References and every subschema's position are kept:

```bash
jsonschemars canonicalize order.json -o order.canonical.json
```

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
