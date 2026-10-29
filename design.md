# json-schema-rs design and architecture

This document is the **design and architecture knowledge bank** for the json-schema-rs crate. It describes how the library is designed, how each JSON Schema feature is (or will be) implemented, and how the JSON Schema specification defines each feature across draft versions.

- **One section per feature/keyword** (or sub-sections for related keywords). Research uses **only** the local specs under `specs/` (draft-00 through 2020-12)—download them with `make vendor_specs`; they are gitignored and not in the repo. No reliance on the web.
- Related features are grouped (e.g. string constraints under Strings, number constraints under Numbers). Each section can have **Spec version quirks** sub-sections for differences between drafts; we implement per the latest supported spec and may expose version-based config where behavior differs.

Implemented keywords: type (object, string, integer, number, boolean, array), properties, required, title, enum, const, items, additionalProperties, uniqueItems, minimum, maximum, exclusiveMinimum, exclusiveMaximum, multipleOf. Other keywords are documented in the sections below. Every schema position accepts a boolean schema (`true` / `false`). Keys the model does not hold (unimplemented keywords, `x-` extensions, unknown keys) are kept in `JsonSchema::extensions` and round-trip; they do not cause an error. Properties whose schema does not map to a Rust type are omitted from generated structs.

---

## High-level architecture

The crate provides **three tools**:

1. **JSON Schema → Rust struct** (codegen): generate Rust types from a JSON Schema.
2. **Rust struct → JSON Schema** (reverse codegen): generate a JSON Schema from Rust types.
3. **JSON Schema validator**: two inputs—JSON Schema definition and JSON instance—output validation result.

**For every feature we develop, implement it for each of these three tools** where the feature applies.

We have three separate pipelines: Schema→Rust, Rust→Schema, and the validator. Code layout: workspace crates `json_schema_rs/` (library and `jsonschemars` CLI binary) and `json_schema_rs_macro/` (proc-macro `generate_rust_schema!` for compile-time codegen). When adding a new keyword or type, consider: schema model, codegen/validation behavior, tests, and examples.

### JSON Schema validator

The validator takes the same schema model used by codegen and a JSON instance (`serde_json::Value`) and returns `Result<(), Vec<ValidationError>>` (type alias **ValidationResult**). It collects **all** validation errors (no fail-fast) and returns them at the end. Inputs: anything that converts to a `SchemaRef` (`&Schema`, which may be a boolean document, or `&JsonSchema`) and `&Value`. Output: `Ok(())` when valid, `Err(errors)` when invalid. `validate` builds the schema's `Registry` on every call; `validate_with_registry(&registry, instance)` reuses one built once (and is the only way to give `$ref` other documents). A schema whose registry cannot be built fails every instance with one error at the root: `InvalidRef` for a `$ref` that does not resolve, `InvalidSchema` for anything else (a duplicate `$id`, a metaschema requiring an unknown vocabulary).

**Supported keywords:** `$schema` (per resource, including custom metaschemas and their `$vocabulary`), `$id` / draft-4 `id` (resources and base URIs), `$anchor` and `$dynamicAnchor` (as plain anchors), `$ref` (any URI reference, resolved through the [schema registry](#schema-registry-ref-resolution); keywords next to `$ref` are not evaluated yet, #28), `$defs` / `definitions` (container only), `type` (all seven types, as a set), boolean schemas at every position, `required`, `properties` (recursive), `additionalProperties` (boolean or schema), `enum`, `const`, `allOf`, `anyOf`, `oneOf`, `pattern`, numeric bounds (`minimum`, `maximum`, `exclusiveMinimum`, `exclusiveMaximum`, `multipleOf`, compared exactly; see *8. Numbers*), length bounds, `uniqueItems`, and selected `format`s. Type-specific keywords apply by the instance's kind, whatever `type` says or whether it is present: `required`/`properties`/`additionalProperties` to objects, length/`pattern`/`format` to strings, `items`/count bounds to arrays, numeric keywords to numbers, `uniqueItems` to arrays. A `type` mismatch is reported next to every other keyword the instance breaks (all errors, no fail-fast). The validator and codegen reuse the same `JsonSchema` struct; one parse, one model. A compiled validator (e.g. tree of validator nodes) can be added for performance; the same schema model would be used. **Current performance:** in the benchmark harness our validator is the fastest in none of the 22 in-process validation time cells; the `jsonschema` crate wins 21 and ties 1, and is typically 5–60× faster (e.g. 148 ms vs 7.5 ms on a 6 MB instance), because `validate` re-walks the parsed schema on every call; the compiled-schema IR (#8) addresses this.

**Validation errors:** Each `ValidationError` variant includes instance context (actual value, count, length, or "got" type) and the schema constraint where applicable. Display messages are one line per error and actionable (e.g. `/: value "pending" not in enum (allowed: "open", "closed")`; `/: array has 2 item(s), minimum is 3`; `/: value 15 is above maximum 10`). Messages are never truncated; full allowed sets, values, and lengths are shown.

### Official JSON Schema Test Suite

We run the [JSON Schema Test Suite](https://github.com/json-schema-org/JSON-Schema-Test-Suite) as a **gating test** in `make test` (`json_schema_rs/tests/json_schema_test_suite.rs`, a `libtest-mimic` harness with `harness = false`). Design:

- **Scope.** Drafts 4, 6, 7, 2019-09 and 2020-12, required and `optional/` (including `optional/format/`). Each draft directory is parsed with that draft's `SpecVersion` pinned in the settings, so draft-specific behaviour takes effect in the gate as soon as the library implements it. Files under `optional/format/` are parsed with `FormatMode::Assertion`, because those tests are written for implementations that assert `format`.
- **One reported test per suite test**, named `<draft>/<file> :: <group> :: <test>`, so a failure names the case (`cargo test --test json_schema_test_suite -- <filter>` runs a subset). Every draft/file/group/test key in the scope is unique; a dedicated case fails if a suite update ever breaks that.
- **Strict parsing** (`disallow_unknown_fields(true)`): a schema with a key its draft does not define fails at parse time ("schema not supported"). A keyword the draft defines but this library does not implement yet (`not`, `if`, `patternProperties`, ...) parses and is ignored by the validator, so its cases pass or fail on their merits and the failures sit in the manifest until the keyword lands. Some suite schemas use keys their own draft does not define (`$comment` in draft 4 and 6, `const` in draft 4); strict parsing rejects those, and those cases are known failures by design. A panic in the parser or validator fails that case, not the run.
- **Known-failure manifest.** `json_schema_rs/tests/suite_xfail.json` (draft → file → group → test descriptions, sorted) lists every case that fails today. The run fails on any **unexpected failure** (a case not in the manifest fails), any **unexpected pass** (a listed case now passes), and any **stale entry** (the manifest names a case that is not in the suite). The manifest can therefore only change deliberately: a fix removes entries, and a reviewer sees the diff. `make update_suite_xfail` rewrites it from the current results (`JSON_SCHEMA_RS_SUITE_XFAIL_UPDATE=1`); review that diff before committing, since it also records new failures. Cases marked `[xfail]` in the test output are expected failures.
- **Pinned suite.** `make vendor_test_suite` clones the suite at the commit pinned in `research/scripts/clone-json-schema-test-suite.sh` (`SUITE_REV`), because the manifest is keyed to that revision. Moving to a newer suite is a deliberate change: bump `SUITE_REV`, run `make vendor_test_suite update_suite_xfail`, review the manifest diff, and refresh `make differential`.
- **Missing suite is a hard failure** with a message to run `make vendor_test_suite`.
- **Remotes and registries.** Each group's schema is validated through a `Registry` whose retriever serves the suite's `remotes/` folder under `http://localhost:1234/`, as the suite specifies. A group whose schema has its own `$schema` is parsed under that (not the directory's pinned draft), so a custom metaschema (`vocabulary.json`, `optional/format-assertion.json`) is found through the registry; one without `$schema` is pinned to its directory's draft.

### Known failures

Every open bug issue has at least one regression test that asserts the **correct** behaviour, so the refactor is measured by those tests flipping, and nothing that passes today can silently break. Validator, parser and codegen bugs live in `json_schema_rs/tests/known_failures.rs`; derive bugs in `json_schema_rs_macro/tests/known_failures.rs` (separate crates, so each file carries its own copy of the helpers it needs; while no derive bug has a test, its helper carries `#[expect(dead_code)]`, which turns into an error once a test uses it, so the attribute gets removed). Bugs the official suite also covers (#28, #31, #32) have both a dedicated test and manifest entries.

- `known_failure(issue, body)` runs the body under `catch_unwind`: the test passes while the body panics and fails with "#N now behaves correctly" as soon as it completes, the same "unexpected pass" rule as the suite manifest.
- `known_failure_in_child_process(issue, name, body)` is for bugs that abort the process (stack overflow: #4, #36) or never terminate (codegen hang: #35). The test re-runs itself as a child process filtered to `name` (selected by `JSON_SCHEMA_RS_KNOWN_FAILURE_CHILD`), with a 10 s timeout. A crash, a non-zero exit, or a timeout is "still failing"; a clean exit that printed the pass marker is an unexpected pass; a clean exit without the marker (a wrong `name`) fails the test rather than counting as "still failing". Two harness tests pin both of those outcomes.
- Repro shapes are chosen so a hang holds memory flat (the #35 self-reference through an array of `#` allocates gigabytes while hanging, so it is left to #9's golden tests).
- **When a fix lands:** unwrap the issue's tests into plain `#[test]`s, tighten them to full golden values where the issue asks for it (codegen tests assert only that generation succeeds or which structs are emitted, because the emitter's exact output is #9's to define; #4 checks termination only), and remove the issue's entries from `suite_xfail.json`.
- **Naming:** a test is named for the behaviour it checks (`integer_accepts_zero_fraction_float`), never for the issue. The issue number is the wrapper's first argument and the `// --- #N: ... ---` comment that groups the issue's tests; both go away on unwrap, so the test keeps a name that stays true.
- **New bugs:** add a test with the wrapper when filing, unless it is fixed in the same change.

### Benchmark harness

Performance is measured by the **`json_schema_rs_benchmark`** workspace crate (top-level, next to `json_schema_rs`; `publish = false`), driven by a single `make benchmark` target (`research_benchmark` is a backwards-compat alias). It compares our library against competitor tools on shared fixtures and reports a winner per (tier, operation, metric) only when the data supports one. Design:

- **Two tiers, measured independently.**
  - **In-process** (the head-to-head comparison): every tool is linked into one process as a Rust library — our lib, the Stranger6667 `jsonschema` crate, `boon`, and `typify` — and called on in-memory inputs, so process startup and file I/O are excluded. Time comes from **criterion** (`benches/cross_tool.rs`; median ± standard deviation per call, sub-microsecond resolution). Peak memory is the extra heap each operation allocates above what was in use at its start, recorded once (after a warmup run) under a counting global allocator (`src/bin/heap_profile.rs`, `peak_alloc`; the workspace forbids `unsafe`, so we do not write our own `GlobalAlloc`). Every tool runs under the same allocator in the same process, so the comparison is exact and deterministic.
  - **CLI** (what a command-line user experiences): every tool's binary runs end to end — startup, file I/O, parse, compile, generate/validate, exit — timed by **hyperfine** (`-N`, warmup runs, repeated runs; median ± standard deviation) with each run's peak RSS. typify is invoked as the `cargo-typify` binary directly, never through `cargo`, so the cargo dispatcher's process tree does not pollute its time and memory. Each cell is a separate hyperfine invocation, because hyperfine's per-run peak RSS carries over across commands within one invocation.
- **Operations.** `codegen` (schema text → Rust source text), `schema_compile` (schema text → ready-to-validate schema; in-process only), `validate_valid` and `validate_invalid` (compiled schema + parsed instance → verdict). Each validator is called through its **full error-report API** in both verdict paths — ours `validate_with_registry` (the `Registry` is built in `schema_compile`, as the competitors compile), `jsonschema`'s `iter_errors` collected, `boon`'s `Schemas::validate` — with default settings. typify runs exactly as the `cargo-typify` CLI does by default (struct builders on; `TypeSpace` → `syn` → `prettyplease`).
- **Only correct cells are measured.** A cell is timed only when the tool is correct on it: codegen succeeds, the schema compiles, the valid instance is accepted, the invalid instance is rejected. Panics are caught. Anything else becomes a compatibility-matrix failure with a one-line reason, so a fast failure never looks like a fast result.
- **The benchmark is hermetic.** A fixture whose schema has an external `$ref` (one not starting with `#`) is not measured in either tier and is reported as "not measured: schema needs external documents". Without network access a tool would fail to resolve the reference; with it, a tool such as `jsonschema-cli` would spend the measurement downloading. Either way the number would not describe the operation. Beyond `$ref`, a tool may fetch other documents (e.g. `jsonschema-cli` downloads an unrecognised `$schema` meta-schema such as `http://json-schema.org/schema#`), so each CLI-tier correctness probe runs with network access denied (macOS `sandbox-exec`, Linux `unshare -rn`); a tool that needs the network fails its probe and is not timed. Timed runs are not wrapped, because the sandbox adds several milliseconds per process. In-process, the linked crates are built without HTTP resolution, so they cannot fetch.
- **Statistics and winners.** Every measurement carries median, standard deviation, min, max and sample count. A cell's best tool wins outright only when at least two tools were measured and its lead over the runner-up exceeds both 3% of its median and the sum of the two standard deviations; otherwise the cell is a tie or uncontested, and it is neither bolded nor counted. A category's champion is the tool with the most outright wins, reported together with "wins / contested cells". A measurement whose standard deviation exceeds 10% of its median is flagged `†` as low-confidence.
- **Required tools (hard fail).** The driver aborts up front with install hints if `hyperfine`, `cargo-typify` or `jsonschema-cli` is missing, rather than silently omit a competitor. Fixture-level incompatibility is different: it is a compatibility failure, not an abort. **Competitors are always their newest stable release (hard fail).** Before measuring, the driver checks that each linked competitor crate in `Cargo.lock` (`jsonschema`, `typify`, `boon`) is the newest stable version on crates.io and that each competitor CLI (`jsonschema-cli`, `cargo-typify`) is that same release, so both tiers measure one release and we never optimize against a target that has already moved. A stale tool aborts the run with the upgrade command. If crates.io is unreachable, the freshness check is skipped with a warning; the CLI-matches-crate check always runs. Tool versions (CLI binaries and the crate versions in `Cargo.lock`) are recorded in the results.
- **Fixtures.** Committed real-world and hand-written fixtures live in `json_schema_rs_benchmark/fixtures/<size>/<name>/` (`schema.json` + `instances/valid/*` + `instances/invalid/*`) at small/medium/large/massive tiers; `manifest.json` records provenance and license. Large synthetic fixtures (`generated/wide-object`, `large-array`, `deep-nesting`, `many-defs`) are produced by `src/bin/gen_fixtures.rs` into `target/benchmark-fixtures/` on every run — deterministic (fixed-seed PRNG, byte-identical output), idempotent, and never committed. They are sized so operations take long enough for timing noise not to dominate (e.g. a 6 MB instance validated end to end). Each invalid instance breaks exactly one rule at its last or deepest point, so a validator that stops at the first error still traverses the whole document. Both tiers discover fixtures through the same code (`src/fixtures.rs`; the CLI tier calls `list_fixtures`), and validate against the first instance by file name. Any **valid** schema our library cannot handle is filed as an issue (e.g. #18).
- **Results are committed.** `make benchmark` writes `results.json` (raw measurements, compatibility, tool versions) and `results.md` (methodology, category winners, one table per tier/operation/metric, compatibility matrix) into the crate, so a PR reviewer sees the performance impact in-diff. Per `CLAUDE.md`, **a PR is not complete until the harness has been run and these files are up to date.**

### Differential-testing harness

Correctness relative to the ecosystem is measured by the **`json_schema_rs_differential`** workspace crate (top-level; `publish = false`), driven by `make differential`. It answers "do we return the same verdicts as mature implementations, and are our error messages as useful?" — the in-repo suite test above only checks us against the suite's expectations, so it cannot tell an ambiguous case from our bug. Design:

- **Bowtie is the source of truth for verdicts.** [Bowtie](https://github.com/bowtie-json-schema/bowtie) is the JSON Schema organization's cross-implementation harness: every implementation runs in a container and speaks a line-delimited JSON protocol over stdio. We ship our own harness (`src/bin/bowtie_harness.rs`, image `localhost/json-schema-rs:latest` built by `Dockerfile` from the repo root) so Bowtie runs us next to competitors across languages: Stranger6667/jsonschema and boon (Rust), ajv and hyperjump (JS), python-jsonschema, networknt (Java), JsonSchema.Net (.NET), Blaze (C++), and santhosh-tekuri/jsonschema (Go). The list lives in `scripts/run_differential.sh` (`COMPETITORS`); competitors that do not support a draft are left out of that draft's run (`bowtie filter-implementations`), and an implementation that fails to start aborts the run.
- **Scope.** The official suite for draft 4, 6, 7, 2019-09 and 2020-12, required and optional tests reported separately (Bowtie refuses a directory named `optional`, so the driver runs a flattened copy; Blaze runs on the required suite only, because its harness sends a malformed response whenever a schema has a `$ref` it cannot resolve, which crashes or hangs Bowtie). Every benchmark fixture (real-world and massive schemas) is also run through `bowtie validate` with the manifest's expected verdicts.
- **Coverage vs correctness.** Our harness parses strictly (`disallow_unknown_fields(true)`). A schema we cannot parse is reported to Bowtie as a **skip** with a structured reason (`unsupported keyword "<key>" at <path>` for a key the draft does not define, `unsupported schema: ...` for any other parse error, per-test `unsupported $ref: ...`), never as a verdict. A keyword the draft defines but we do not implement is parsed and ignored, so it shows up as wrong verdicts (correctness), not as skips. Skips measure **coverage**; a real verdict that is wrong measures **correctness**; the report also ranks the unsupported reasons by tests affected, which is the roadmap signal for keyword work.
- **Majority diff.** For every test where we returned a verdict, the competitors' strict-majority verdict (skipped/errored competitors excluded; ties are "no consensus") is compared to ours, and each disagreement is listed with whether the majority matches the suite's expected value (likely our bug vs ambiguous case).
- **Error messages are compared in-process.** Bowtie's protocol carries only valid/invalid flags, so `differential_report` links our library, the `jsonschema` crate and `boon` and records each one's full messages side by side for a deterministic curated set (the first invalid test we support per draft 2020-12 suite file, plus every invalid fixture instance).
- **Results are committed; raw reports are not.** Raw Bowtie JSONL (and logs) go to the gitignored `json_schema_rs_differential/reports/`; `results.json` and `results.md` are committed and byte-deterministic for the same inputs.
- **Prerequisites.** `bowtie` (`uv tool install bowtie-json-schema`), a running container engine (Docker), and `make vendor_test_suite`. The driver checks all three up front.
- **Relation to the in-repo suite test.** `json_schema_rs/tests/json_schema_test_suite.rs` is the Docker-free, cargo-native regression gate in `make test`, with its known-failure manifest; the differential harness is the cross-implementation view. Both run on the same pinned suite commit.

### Our top values

These drive design decisions and how we rank competitors:

- **Spec-adherence**: behavior matches the JSON Schema specification(s) we support.
- **Determinism**: same input always produces same output; stable ordering (e.g. alphabetical).
- **Performance**: efficient algorithms and data structures; avoid unnecessary work.
- **Testability**: core APIs work with in-memory writers (e.g. `Vec<u8>`) and test doubles, not only file I/O.
- **Benchmarks**: we maintain benchmarks so we can measure and guard performance.
- **Idiomatic Rust**: Code adheres to idiomatic Rust conventions, the Rust API Guidelines, and standard library patterns. We prioritize clarity and maintainability over brevity or non-Rust idioms from other languages.

### Design principles

- **Testability-first**: Core APIs should work with writers (e.g. `Vec<u8>` or `Cursor<Vec<u8>>`) so tests avoid file I/O.
- **Deterministic output**: Use stable ordering (e.g. `BTreeMap` for alphabetical struct and field ordering). Same input always produces same output.
- **Schema model**: Only model schema fields we need. Use serde with `#[serde(default)]` and `Option` for optional keys.
- **Errors**: Use a custom error enum with manual `Debug`, `Display`, `Error`, and `From` impls (no thiserror unless the project adopts it).
- **No unbounded recursion**: Use an explicit stack (or queue) and iterative loops instead of recursive calls so depth is limited by heap, not call stack. Avoids stack overflow on deeply nested schemas or instances. This applies to every graph algorithm and tree walk (reference crawling, cycle detection, dedup, emission order, `Drop`, error rendering). The derive's schema generator (reverse codegen) also descends natively, bounded by the number of distinct named types plus the container nesting written in the source, both fixed at compile time (see *Rust struct → JSON Schema*). The one exception for *input-driven* depth is the validator's evaluator: it recurses (as every fast JSON Schema validator does) and carries a depth counter that returns a typed `DepthLimitExceeded` error before the native stack can overflow. That limit is a crash guard sized from a stack-usage measurement on a 2 MiB thread, not a policy limit. Rationale and competitor comparison: `research/synthesis.md` §5 and open item O7.
- **Prefer `..Default::default()` when constructing large structs**: When constructing structs with many optional fields (e.g. `JsonSchema`), set only the fields that matter for the context and use `..Default::default()` for the rest. Improves clarity about intent.

### Schema model: struct vs enum

We represent the in-memory schema as a **struct** (`JsonSchema`) with optional fields, not as a Rust enum of schema subtypes (e.g. `ObjectSchema | StringSchema | ...`). Rationale: the JSON Schema spec defines a schema as a **single JSON object with optional keys**; a struct mirrors that shape and keeps deserialization simple. By contrast, `serde_json::Value` is an enum because a JSON *value* is exactly one of several mutually exclusive kinds (Null, Bool, Number, String, Array, Object)—a different domain. Competitor Rust libraries (e.g. schemafy, typify/schemars) use either a typed struct or an untyped Value wrapper; none use an enum of schema subtypes. Using an enum would complicate deserialization and duplicate shared metadata (e.g. title) across variants without clear benefit.

**Type-specific keywords are orthogonal, so they cannot be split by type.** A per-type enum (`String { minLength, .. } | Number { minimum, .. }`) would forbid `minLength` next to `minimum`, but JSON Schema allows mixing them: each type-specific keyword applies to instances of its type and ignores the rest, with or without `type`. `{"minLength": 2, "minimum": 3}` is a valid schema meaning "strings need at least 2 characters, numbers must be at least 3", and `{"type": ["string", "number"], ...}` needs both sets at once. A one-variant-per-type enum cannot represent these without a catch-all variant that re-imports the flat field bag. Exhaustive per-keyword dispatch belongs in the compiled IRs (#8 validator, #9 codegen), which lower each keyword to an enum variant and `match` exhaustively; the typed model is their input (research/synthesis.md §1).

What the struct holds instead:

- **`type` is a `TypeSet`** (`json_schema/type_set.rs`): a `u8` bitset of the seven `JsonType`s (`array`, `boolean`, `integer`, `null`, `number`, `object`, `string`). `type_: Option<TypeSet>`; `None` is "absent". The string form is a set of one; the array form keeps every entry (duplicates collapse, which the metaschemas forbid but whose meaning is clear); `[]` is the empty set and matches nothing. An unknown name (`"strng"`, draft 3's `"any"`) is a parse error naming the seven valid names. `TypeSet::matches(instance, integer_semantics)` is the type check (a number matches `integer` when it is an integer under the dialect). It serializes as a string for one type and an array in name order otherwise, so a round trip normalises the written order.
- **`Schema = Bool(bool) | Object(Box<JsonSchema>)`** at every subschema position (`properties` values, `items`, `additionalProperties`, `$defs` / `definitions`, `allOf` / `anyOf` / `oneOf`) and for a whole document (`Schema::new_from_*`, `TryFrom`). `true` accepts everything (like `{}`), `false` nothing. Accepted in every draft, draft 4 included (which defines it only for `additionalProperties`), since its meaning does not change. `JsonSchema::new_from_*` still parses a document but requires an object (`JsonSchemaParseError::BooleanRoot` otherwise), for callers such as codegen that need one. `SchemaRef<'a> = Bool | Object(&JsonSchema)` is the borrowed form the validator takes. Consumers that describe a shape (codegen) read a boolean schema as `{}` through `Schema::object_or_empty`, so a `true` / `false` property is omitted like a typeless one and `items: true` gives `Vec<serde_json::Value>`.
- **`extensions: BTreeMap<String, Value>`** holds every key the struct has no field for: keywords the model does not hold (`not`, `if`, `patternProperties`, ...), `x-` extensions and unknown keys. It is flattened back on serialize (after the fields, in key order), so parse → serialize loses nothing but the written key order and `type` array order. `JsonSchema` has a hand-written `Deserialize` (a `Field` list generated from one macro table plus a catch-all) rather than `#[serde(flatten)]`, because flatten buffers the whole object into serde's `Content` at every nesting level and does not compose with `serde_json`'s `arbitrary_precision` (#39). It accepts only a JSON object, so an array where a schema is expected (`{"items": [{}]}`, draft 4–2019-09's tuple form) is "expected a schema (an object or a boolean)"; a repeated key is a `duplicate field` error, as with a derived `Deserialize`.
- **Numbers** are exact `serde_json::Number`s (#5); draft quirks are normalised at parse time (*Dialects*). Parsing never mutates the caller's value.

### Settings and spec version

**JsonSchemaSettings** control how JSON Schema definitions are ingested (parsed). Use `JsonSchemaSettings::default()` for default settings (equivalent to `JsonSchemaSettings::builder().build()`), or `JsonSchemaSettings::builder()` to construct custom settings; options include `disallow_unknown_fields` (when `true`, reject any schema object with a key its draft does not define; see *Dialects*, *Parsing*), `spec_version`, `format_mode` and `unknown_schema_uri` (see *Dialects*). **SpecVersion** is an enum with one variant per vendored spec (Draft00 through Draft202012); `default_schema_settings()` returns settings that pin that version. Schema is parsed via **TryFrom** for default settings (e.g. `JsonSchema::try_from(json_str)`, `path.try_into()`), or constructor-style **`JsonSchema::new_from_str`**, **`JsonSchema::new_from_slice`**, **`JsonSchema::new_from_serde_value`**, **`JsonSchema::new_from_reader`**, **`JsonSchema::new_from_path`** (all take `&JsonSchemaSettings`) for custom settings. I/O errors from reader/path are reported as **JsonSchemaParseError::Io**. The CLI and macro build settings from flags or builders and pass them through. **CodeGenSettings** are language-agnostic codegen options (e.g. `model_name_source`). Use `CodeGenSettings::builder()`. The **CodeGenBackend** trait (in `code_gen/mod.rs`) takes `&CodeGenSettings` in `generate`. The Rust backend and `generate_rust` live in `code_gen/rust_backend.rs`; settings live in `code_gen/settings.rs`. CLI prefixes: `jss-` (JSON Schema Settings), `cgs-` (codegen settings); future Rust-specific: `cgs-rs-`.

### Schema registry ($ref resolution)

`$ref` is resolved through one **`Registry`** (`json_schema/registry.rs`), shared by the validator and codegen (and, for #50, by the derive). Background and competitor survey: `research/synthesis.md` §1 (three layers) and §3.

- **What it holds.** Documents (the root, borrowed or owned, plus every document loaded for a `$ref` or a custom `$schema`), the **resources** (one per document retrieval URI and one per subschema with `$id` / draft-4 `id`: base URI, location, dialect, anchors, dynamic anchors), and, per resource, each `$ref` as written mapped to its target. Schemas the typed model keeps as raw JSON that a `$ref` targets are parsed once into owned "detached" schemas. The typed model plus `extensions` is the document store: it keeps every key, so no separate raw `Value` copy is needed.
- **Built once, immutable.** `Registry::new(&schema)` (defaults) or `Registry::builder()` (`base_uri`, `document(uri, value)`, `retriever(impl Retrieve)`, `settings`, then `build(&schema)` or `build_owned(schema)`). Building is an iterative loop: discover a document (explicit-stack walk), resolve its queued `$ref`s, load any document a `$ref` names that is not known yet, discover that, repeat. Nothing is fetched after `build` returns.
- **Discovery.** Schema positions are the typed fields of `JsonSchema` plus, inside `extensions`, the keywords whose value holds subschemas in the resource's dialect keyword table (`SubschemaPosition`). An unknown keyword or a data keyword is not a schema position, so an `$id` / `$anchor` there is not an identifier (suite `unknownKeyword.json`, `optional/id.json`). Pointer paths are kept in a parent-linked arena and turned into strings only for resources and anchors.
- **Identity.** A resource URI is absolute and fragment-free and in normal form, through the `fluent-uri` crate: RFC 3986 / RFC 3987 syntax (an invalid `$id` is `RegistryError::InvalidUri`, an invalid `$ref` is unresolvable), §5.2 reference resolution, and §6.2.2 normalisation (scheme and host case, percent-escape case and unreserved decoding, dot segments, default port, canonical IPv6), so two spellings of one identifier are one resource. Any scheme works, including URNs; a relative-path `$ref` against a base with no hierarchical path (`urn:…`) does not resolve, a fragment-only one does. No `fluent-uri` type is in the public API: `Retrieve::retrieve` and `Registry::base_uri` use the normalised URI as `&str`, so a `fluent-uri` release cannot force a breaking release here. A root without a retrieval URI gets `json-schema:///` (`DEFAULT_BASE_URI`). A root `$id` makes a second resource for the same location, so anchors are found under the `$id` and the retrieval URI alike. The same URI at two locations is `DuplicateId`; the same anchor at two locations of one resource is `DuplicateAnchor`.
- **Loading.** A URI no resource has is looked up in the embedded standard metaschemas (drafts 4, 6, 7, 2019-09 and 2020-12 with their vocabulary metaschemas, `include_str!` from `json_schema_rs/metaschemas/`), then the documents given to the builder, then the `Retrieve`r. The default `OfflineRetriever` retrieves nothing, so an unknown external URI is a typed error; `HashMap<String, Value>` / `BTreeMap<String, Value>` are in-memory retrievers. A loaded document is parsed with the builder's settings; one without `$schema` takes the referencing resource's draft. File and HTTP retrieval are not built in: a caller supplies them as a `Retrieve` implementation (codegen's cross-file `$ref` is #19).
- **Scopes.** Consumers carry a `Scope` (a resource index). `Registry::enter(scope, schema)` moves into the resource a schema declares with its `$id` (looked up by the `$id` as written, per parent resource); `Registry::resolve(scope, ref)` returns the target schema, its location, and the scope to evaluate it in; `Registry::dialect(scope)` is that resource's dialect.
- **Errors.** `RegistryError`: `UnresolvableRef`, `RefTargetNotSchema`, `DuplicateId`, `DuplicateAnchor`, `Retrieve`, `Parse` (a loaded document or detached value), `UnsupportedVocabulary`, `InvalidUri` (a builder base or document URI that is not absolute, or a `$id` that is not a URI reference). No limits on document count, size or `$ref` hops.

### Codegen backends

Codegen is built around a **swappable backend** trait in **`code_gen/mod.rs`**: input is a slice of `JsonSchema`, **CodeGenSettings**, and output is **GenerateRustOutput** `{ shared: Option<Vec<u8>>, per_schema: Vec<Vec<u8>> }`. The trait `CodeGenBackend` has a single method, `generate(&self, schemas: &[JsonSchema], settings: &CodeGenSettings) -> Result<GenerateRustOutput, Error>`. The **CLI** builds `CodeGenSettings` and `JsonSchemaSettings` from flags and calls the corresponding backend. The only implementation today is **Rust** (`RustBackend` in **`code_gen/rust_backend.rs`**), which emits serde-compatible Rust structs; when dedupe is enabled (default), structurally identical object schemas within and across schemas are emitted once in a shared buffer. The public API is `generate_rust(schemas: &[JsonSchema], settings: &CodeGenSettings) -> Result<GenerateRustOutput, Error>`; callers use `output.per_schema` (one buffer per schema) and optionally `output.shared`. **Adding another language:** implement `CodeGenBackend` for a new type (e.g. `PythonBackend`) in a new module (e.g. `code_gen/python_backend.rs`), add a match arm in the CLI’s `run_generate` for the language name (case-insensitive), and update the "supported" text in the unsupported-language error message (e.g. "supported: rust, python").

**Codegen entry points:** (1) **CLI** — one or more INPUTs (file paths, directory paths recursively searched for `.json`, or `-` for stdin); required `-o` output directory. CLI flags: `--jss-disallow-unknown-fields`, `--cgs-model-name-source title-first|property-key`, `--cgs-dedupe-mode disabled|functional|full`. Schema file ingestion uses `JsonSchema::new_from_slice` with `JsonSchemaSettings` built from flags; each failure is logged to stderr and the command exits with failure after all files have been attempted (no output is written if any ingestion fails). Generated output path components are **sanitized** so that file and directory names are valid Rust identifiers. When dedupe produces shared structs, the CLI writes them to **`shared.rs`** in the output directory and adds `pub mod shared;` to the root `mod.rs`; per-schema files reference shared types via `super::shared::Typename` (or chained `super::` for nested output) so generated code compiles whether `-o` points to crate root or a submodule. (2) **Library** — `generate_rust(schemas, &CodeGenSettings::builder().build())` returns **GenerateRustOutput** (`shared`, `per_schema`). Use `output.per_schema` for one buffer per schema; use `output.shared` when present for the shared definitions. Parse schemas with `JsonSchema::try_from(json)` (or `.try_into()`) for default settings, or `JsonSchema::new_from_str`, `new_from_slice`, `new_from_serde_value`, `new_from_reader`, `new_from_path` with `&JsonSchemaSettings::default()` or custom settings. (3) **Macro** — the `json-schema-rs-macro` crate provides `json_schema_to_rust!(...)`, which runs at compile time and **inlines** the generated Rust at the call site. Use `json_schema_rs_macro::json_schema_to_rust`. The macro builds `JsonSchemaSettings` and `CodeGenSettings` via their builders (no args) and calls `RustBackend::generate(&schemas, &code_gen_settings)`; when `output.shared` is present it emits a `shared` submodule; module name from file stem for paths, or `schema_0`, `schema_1`, … for inline. Macro-related tests and fixtures live in the macro crate; the main crate has no macro-specific tests. Consumers add both `json-schema-rs` and `json-schema-rs-macro` and use `json_schema_rs_macro::json_schema_to_rust`. A re-export from the main crate would require the main crate to depend on the macro crate, which would create a cyclic workspace dependency (the macro crate depends on the main crate for codegen).

**Model deduplication.** When **DedupeMode** is not **Disabled** (default is **Full**), the Rust backend deduplicates structurally identical object schemas **within** a single schema and **across** multiple schemas. One Rust struct is generated per equivalence class; shared structs are emitted in the **shared** buffer (e.g. `shared.rs`), and per-schema buffers contain only structs used in that schema plus `pub use super::shared::...` (or chained `super::` for nested files) for shared types they reference. Equality is **deep** (nested objects must match under the same rules). **Functional** mode compares only pivotal/functional data (type, properties, required, title, constraints); **Full** mode also compares non-functional fields (e.g. `description`). Dedupe uses a **DedupeKey** (built from `JsonSchema` and mode) with **Ord** + **Eq** in a **BTreeMap** for deterministic, idempotent output. Canonical struct name is the first occurrence's name (by schema index and traversal order). When dedupe is **Disabled**, `shared` is always `None` and `per_schema` is the same as the previous one-buffer-per-schema behavior.

**Codegen tests: compile and deserialize.** We verify that generated Rust compiles and that serde deserialization works by writing generated code into temporary Cargo crates (edition 2024, lib + binary), running `cargo build`, then running a binary that deserializes a fixed JSON string into the generated type(s). Integration tests in **`json_schema_rs/tests/integration.rs`** use a **single workspace test** (`generated_rust_build_and_deserialize_all_scenarios`): one temp workspace with one package per scenario (unique crate names), `cargo build --workspace` once with **shared `CARGO_TARGET_DIR`** (e.g. `CARGO_TARGET_DIR/integration_codegen`) so path dependencies (json-schema-rs, json-schema-rs-macro) are built once and reused, then `cargo run -p <name>` for each scenario. The workspace has **eleven** members: **kitchen_sink** (one schema combining many features—required/optional primitives, arrays, uniqueItems, min/max items, nested object, description, string min/max length, hyphenated property, enum—with one `main` asserting the same behavior as the former single-feature scenarios), **round_trip** (one crate with seven modules, each running one round-trip assertion; `description` checks that schema `description` survives forward codegen's `///` docs and the derive), **enum_variants** (one crate with three modules for enum_collision, enum_dedupe, enum_duplicate_values), plus **nested_modules**, **multi_schema**, **hyphenated_paths**, **dedupe_two_identical**, **model_name_source_property_key**, **deep_nesting**, **additional_properties_false** (additionalProperties: false → deny_unknown_fields; main deserializes valid object and asserts extra key fails), and **additional_properties_schema** (additionalProperties: string schema → map field; main deserializes and round-trips via serialize/deserialize). Each scenario has one assertion; failure messages include the scenario name. Temp crates that compile generated code with the ToJsonSchema derive add **json-schema-rs** and **json-schema-rs-macro** as path dependencies so the derive and trait resolve.

### Rust struct → JSON Schema (reverse codegen)

**Entry points:** (1) **Generated structs** — every struct emitted by forward codegen (library, CLI, macro, build+deserialize) has `#[derive(..., json_schema_rs_macro::ToJsonSchema)]` and optional container/field attributes (e.g. `#[json_schema(title = "Root")]`). No stored schema constant; the derive builds the schema from the type and attributes. (2) **Hand-written structs** — use `#[derive(ToJsonSchema)]` from `json_schema_rs_macro` with optional `#[json_schema(...)]` attributes. Consumers need **json-schema-rs** (trait, `JsonSchema` type) and **json-schema-rs-macro** (derive) in scope; generated code uses the full path `json_schema_rs_macro::ToJsonSchema` in the derive list.

**Trait and serialization:** The **ToJsonSchema** trait (in **`reverse_code_gen`**) has `fn json_schema() -> JsonSchema` (a standalone root schema) and `fn json_schema_with(generator: &mut SchemaGenerator) -> JsonSchema` (the schema as a subschema of the document being built). The default `json_schema_with` inlines `json_schema()` through `SchemaGenerator::inline_subschema`, which hoists any nested `$defs` into the current resource's `$defs` and drops `$schema` (a schema with `$id` is embedded whole instead; see *Schema resources*), so a hand-written impl only needs `json_schema`. Containers (`Option`, `Vec`, `HashSet`, `BTreeMap<String, _>`, `Box`) pass the generator through to their element type, and their `json_schema()` is `root_schema_for::<Self>()` (a fresh generator plus the `$defs` it collected).

**Generator context (schemars model, #4):** `SchemaGenerator` owns the `$defs` entries and a **pending set** of named types whose body is being built. Every derived struct and enum is a named type: its `json_schema_with` calls `generator.named_subschema(Self::schema_id(), name, body)`, which returns a `$ref` and builds the body the first time that identity is seen. An identity already in the pending set returns its `$ref` without building, so a cycle of any length (`A → B → A`, `A → B → C → A`, self-reference) terminates; each type's body is built at most once per generator. The derived `json_schema()` builds its own body as the root (`into_named_root_schema`) and adds `$schema`; when a nested type refers back to the root, the root body is also placed in `$defs` so that `$ref` resolves. `$defs` entries never carry `$schema` (2020-12 core §8.1.1: it must not appear in a subschema that is not a resource root). The derive writes the body once, in a hidden inherent fn `__json_schema_rs_body`, used by both trait methods. Whether a field's type is a named type is decided by the type's own impl, not by inspecting field tokens, so `uuid::Uuid` (a library impl with the default `json_schema_with`) inlines as `{"type":"string","format":"uuid"}` with no name heuristic, while a user `struct Uuid` gets a `$defs` entry.

**`$defs` identity and naming (#24):** entries are keyed by **identity**, never by name. A named type's identity is the hidden trait fn `ToJsonSchema::schema_id()`: the derive returns `concat!(module_path!(), "::", "Name")`, which expands where the type is defined, so `a::Config` and `b::Config` are two entries (a proc macro only sees the path written at the field, which cannot tell whether `Config` and `crate::a::Config` are the same type; schemars uses the same `schema_id` approach). The default `schema_id` is `std::any::type_name::<Self>()`, used only by hand-written impls that call `named_subschema`. A `$defs` entry hoisted from a hand-written impl's standalone schema (`inline_subschema`) is its own identity, ordered by first appearance. **Final names are assigned in one pass after generation** (`into_root_schema`), because `$ref` strings are emitted while bodies are still being built and identities are only known once the walk is done: `named_subschema` returns a provisional `$ref` (`#/$defs/#<index>`; a second `#` is not a valid URI character, so no hand-written `$ref` can match it), and `into_root_schema` assigns names, then rewrites every provisional `$ref` with an explicit-stack walk. Naming: a name used by one entry stays bare; when several entries share a name, **every** one gets a numeric suffix (`Config0`, `Config1`) in identity order (named types by `schema_id`, then hoisted entries), skipping a suffixed name that is already taken (a type actually named `Config0` keeps its name and the collision group continues at `Config1`). Nothing is first-wins and no schema is dropped. Hoisted entries: `inline_subschema` takes the `$defs` out of the standalone schema at every depth and rewrites its `$ref`s by JSON Pointer (`#/$defs/Outer/$defs/Inner`, and refs into a def such as `#/$defs/Inner/properties/x`); a bare `#/$defs/Inner` with no top-level `Inner` resolves to the nested `Inner` when exactly one exists (the shape the pre-generator derive emitted), otherwise it is left as written. Inlining an equal standalone schema again reuses its entries (so a recursive hand-written schema used twice is defined once), and before naming, a hoisted entry whose name and body equal another entry's (a named type first, else the earlier hoisted one) is merged into it, repeated until nothing merges, so identical defs from different hand-written impls collapse while conflicting ones are both kept and suffixed.

**Schema resources (`$id`, #46):** a type with `#[json_schema(id = "...")]` is its own schema resource, and inside a resource a fragment `$ref` such as `#/$defs/Leaf` resolves against that resource's `$id`, not the document root (2020-12 core §8.2.1, §9.2.1). The generator bundles per Appendix B.1: an embedded resource behaves exactly as it would as a separate document. The derive knows about `id` at compile time and calls dedicated methods (`resource_subschema`, `into_resource_root_schema`) instead of `named_subschema` / `into_named_root_schema`. Rules:
- **Own scope.** A resource's body is built in a child scope (`Scope`: its own definitions, pending set and inline cache), so the named types it refers to land in the resource's own `$defs`, and the scope is finalized (names assigned, provisional refs rewritten) before the body leaves it. A non-`$id` type used both inside and outside a resource is therefore defined once per scope that uses it. Scopes are a stack on the generator; descent stays bounded because each `$id` type is built once per generator and each named type once per scope.
- **Self-reference is `#`**, from the resource body or from any entry in its `$defs` (they share its base URI). A derived `$id` root uses `#` too and never appears in its own `$defs`; the non-resource root keeps the old rule (placed in `$defs` when referred back to).
- **One copy per `$id`.** Two embedded copies of one `$id` are undefined behaviour, so each `$id` type is embedded once, in the document root `$defs` (keyed and named like any other named type). The document root refers to it as `#/$defs/<Name>`; another resource refers to it by its `$id` as written, which resolves to the embedded resource when the `$id` is an absolute URI (the form 2020-12 recommends; a relative `$id` would resolve against the referring resource's base instead).
- **Cycles across resources** (`A → B → A`): a resource already being built higher up is referred to by its `$id`, so the walk terminates like the pending set does for named types.
- **Hand-written resources.** `inline_subschema` embeds a schema with `$id` whole (`$schema` and `$defs` included). Inside a standalone schema without `$id`, a `$defs` entry with `$id` keeps its own `$defs`, and no `$ref` inside any embedded resource is rewritten when hoisting, since it resolves against that resource. Two uses of the same hand-written resource embed it at both sites.

**Field overrides never force inlining:** every field schema is `<FieldType>::json_schema_with(generator)` with the field attributes applied on top, so an override on a named-type field becomes a sibling of its `$ref` (e.g. `{"$ref": "#/$defs/Address", "deprecated": true}`), and on a `Vec<Node>` field it lands on the array schema (`minItems` beside `items: {"$ref": ...}`). `$ref` siblings are valid from 2019-09; the derive emits 2020-12 only. If an older output dialect is added, overrides beside a `$ref` must be wrapped as `allOf: [{"$ref": ...}]` plus the overrides for draft 7 and earlier, where `$ref` siblings are ignored.

**Serialization:** `JsonSchema` implements **Serialize** and **TryFrom<&JsonSchema> for String** / **TryFrom<&JsonSchema> for Vec<u8>** (and consuming forms); use `String::try_from(&schema)` or `.try_into()` to get JSON. Error type is **JsonSchemaParseError** (wraps `serde_json::Error`). Round-trip: parse schema → generate Rust → compile crate with json-schema-rs + macro → for each generated type call `TypeName::json_schema()` → TryFrom to String/Vec<u8> → parse back → assert equals original (or derived) schema.

**Container/field attributes:** Container: **`#[json_schema(title = "...")]`** (when the schema had a title). Field-level **`#[json_schema(minimum = N, maximum = N)]`** is supported for emitting JSON Schema bounds on a property; N can be integer or float literals (stored as f64). When present, the attribute value overrides the type-derived minimum/maximum (e.g. an `i64` field with `#[json_schema(minimum = 0, maximum = 255)]` emits a schema with those bounds). Field-level **`#[json_schema(min_items = N, max_items = M)]`** is supported for array/set properties (Vec, HashSet, Option<Vec>, Option<HashSet>); when present, the attribute overlays the type-derived schema so the emitted JSON Schema includes minItems/maxItems. Field-level **`#[json_schema(min_length = N, max_length = M)]`** is supported for string properties (String, Option<String>); when present, the attribute overlays the type-derived schema so the emitted JSON Schema includes minLength/maxLength. Field-level **`#[json_schema(pattern = "...")]`** is supported for string properties; when present, the emitted JSON Schema includes the pattern (ECMA 262 regex). Field-level **`#[json_schema(deprecated = true)]`** is supported; when present, the emitted JSON Schema includes `"deprecated": true` for that property. Other field attributes (e.g. `#[json_schema(...)]`) are parsed for future use; only supported schema keywords (type, properties, required, title, minimum, maximum, min_items, max_items, min_length, max_length, pattern, deprecated) are emitted today. Attribute names follow a Serde-style pattern (container vs field). **Recursion in `reverse_code_gen`:** hoisting nested `$defs`, `$ref` rewriting and serialization use iteration + stack; the generator's descent is bounded by the distinct named types and source-level container nesting (see *Generator context* and the design principle above).

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
| Integer with exclusive bounds (narrow type, e.g. u8) | Y | N | Y | Y | Y |
| Integer with a range up to `u64::MAX` (u64, exact bounds) | Y | N | N | Y | N |
| Single required boolean | Y | Y | Y | Y | Y |
| Single optional boolean | Y | Y | Y | Y | Y |
| Type array with null (`["string","null"]` → `Option<String>`) | Y | Y | Y | Y | Y |
| Boolean subschemas (`true`/`false` properties omitted; `items: true` → `Vec<serde_json::Value>`) | Y | Y | Y | Y | Y |
| Dedupe (two identical schemas) | Y | — | Y | Y | — |
| Dedupe nested output (output in submodule) | — | — | Y | Y | — |
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
| Enum with non-Rust-compliant values (e.g. "/8633", "todd.griffin") | Y | Y | Y | Y | — |
| Non-string enum → String fallback | Y | — | — | — | — |
| Const string property (single-value enum) | Y | Y | Y | Y | Y |
| Reverse codegen (every struct ToJsonSchema + attributes) | Y | Y | Y | Y | Y |
| Round-trip (generate → Type::json_schema() → TryFrom → parse → equals) | — | Y | — | Y | Y |
| description (struct, field, enum doc) | Y | Y | Y | Y | Y |
| $comment (round-trip; reverse via comment attribute) | Y | Y | Y | Y | Y |
| deprecated (property, struct; meta-data only; codegen emits #[deprecated]; reverse via #[json_schema(deprecated = true)]) | Y | Y | Y | Y | Y |
| Schema with examples (annotation only; stored, round-trip; dedupe Full/Functional) | Y | — | Y | Y | Y |
| $id (round-trip; id attribute emitted when present; reverse via id attribute) | Y | — | — | — | — |
| Required array property (e.g. Vec\<String\>) | Y | Y | Y | Y | Y |
| Optional array property | Y | Y | Y | Y | Y |
| Array with uniqueItems true (e.g. HashSet\<String\>) | Y | Y | Y | Y | Y |
| Array with minItems/maxItems (schema + emitted attribute) | Y | Y | Y | Y | Y |
| String with minLength/maxLength (schema + emitted attribute) | Y | Y | Y | Y | Y |
| String with pattern | Y | Y | Y | Y | Y |
| allOf (merged object) | Y | — | — | Y | — |
| Array of objects (nested struct) | Y | Y | Y | Y | Y |
| Array of arrays (e.g. Vec\<Vec\<String\>\>) | Y | — | Y | Y | Y |
| anyOf (union enum, property-level + root) | Y | — | Y | Y | Y |
| oneOf (union enum, property-level + root) | Y | — | Y | Y | Y |
| additionalProperties false (deny_unknown_fields) | Y | — | Y | Y | — |
| additionalProperties schema (map field) | Y | — | Y | Y | — |
| Property with default (optional/required + default) | Y | Y | Y | Y | Y |
| $ref to $defs (defs_ref) | Y | Y | Y | Y | Y |

### Rust codegen: name sanitization

All functions that produce valid Rust identifiers (struct names, field names, module names, path components) live in **`json_schema_rs/src/sanitizers.rs`** as a single source of truth.

**Functions and roles:**

- **`to_pascal_case(name)`** — Converts to PascalCase for type names. Uses heck's `ToPascalCase`; our wrapper handles empty → `"Unnamed"` and leading digit → `N{out}`. Non-ASCII → `_` before conversion.
- **`sanitize_struct_name(s)`** — Type/struct/enum names. Uses `to_pascal_case`; then if result is Rust keyword `Self`, appends `_` → `Self_`. Leading digit already prefixed in `to_pascal_case`.
- **`sanitize_field_name(key)`** — Field identifiers (snake_case). Replaces `-` with `_`; invalid chars → `_`; converts camelCase/PascalCase to snake_case via heck's `ToSnakeCase`. Empty → `"empty"`; leading digit → `field_{s}`; single `_` → `"empty"`. Rust strict/reserved keywords (e.g. `type`, `self`) get trailing `_` (e.g. `type_`). Codegen emits `#[serde(rename = "...")]` when field name differs from JSON key. Non-ASCII → `_`.
- **`sanitize_module_name(s)`** — Module names. Replaces `-`, `.`, space with `_`; keeps `[a-zA-Z0-9_]`. Empty → `"schema"`; leading digit → `schema_{s}`; reserved `crate`/`self`/`super` → `{s}_mod`. Non-ASCII → `_`.
- **`sanitize_path_component(component)`** — File stem or dir name for output paths. Replaces `-` and non-`[a-zA-Z0-9_]` with `_`. Empty → `"schema"`; leading digit → `_{s}`. Non-ASCII → `_`.
- **`enum_variant_name_from_value(s)`** — Enum variant names (UpperCamelCase). Normalizes input first: invalid identifier chars (`/`, `.`, etc.) → `_`, trim; then `to_pascal_case`. Leading digit → `E{suffix}`; keyword `Self` → `ESelf`; empty → `EUnnamed`. Result is always valid Rust.
- **`enum_variant_names_with_collision_resolution(values)`** — When multiple values map to the same variant base (e.g. `"Accept"` and `"accept"` both → `Accept`), appends numeric suffix without underscore (`Accept0`, `Accept1`) to preserve UpperCamelCase per [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/naming.html).
- **`sanitize_output_relative(relative)`**, **`module_name_from_path(path)`** — Build on the above.

**Rules summary:**

| Rule | Type/struct | Field | Module | Path component |
|------|-------------|-------|--------|----------------|
| Empty | `Unnamed` | `empty` | `schema` | `schema` |
| Leading digit | `N{out}` | `field_{s}` | `schema_{s}` | `_{s}` |
| Invalid/non-ASCII | `_` (in PascalCase input) | `_` | `_` filtered | `_` |
| Rust keyword | `Self` → `Self_` | keyword → `{kw}_` | `crate`/`self`/`super` → `{s}_mod` | — |

**Stability guarantee:** Sanitizer output is deterministic and intended to be stable across versions. Any change will be documented and rare (e.g. security or spec compliance). Unit tests lock golden input→output pairs (e.g. `"type"` → field `type_`, struct `Type`; `"self"` → struct `Self_`). The switch to heck for snake_case (field names, default function names) was an intentional fix for idiomatic Rust; regenerated code will have different (correct) identifiers.

**Default function naming:** When a property has a custom default value (schema default ≠ Rust type default), codegen emits a module-level function and `#[serde(default = "fn")]`. Function names use snake_case for both struct and field (e.g. `default_valerie_hunter_todd_griffin` for struct `ValerieHunter` and field `toddGriffin`), per [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/naming.html).

**Competitor comparison:** Typify uses heck + custom sanitize; enum variant uniqueness via replacing non-identifier chars with `"X"`; rust-collisions fixture for keywords. schemafy uses Inflector for Pascal/snake; Rust keywords and invalid identifiers escaped with trailing underscore and serde rename. We use heck (like Typify) for PascalCase and snake_case, a single module, explicit keyword set (strict + reserved from the Rust Reference), and trailing `_` for type (`Self`) and field keywords so generated code is always valid without raw identifiers.

**Duplicate struct names:** When two schemas (or title vs property key) produce the same sanitized struct name, codegen currently keeps the first occurrence and skips the second (“first wins”). Future work may add disambiguation (e.g. numeric suffix) or explicit failure.

---

### Dialects

Behaviour that differs between drafts lives in one place: a **`Dialect`** (`json_schema/dialect.rs`), the effective rule set for a schema document. Draft-dependent code reads the dialect; it never branches on `SpecVersion`.

- **Per-draft table.** `Dialect::for_spec_version(v)` gives each draft's defaults. Each draft's keyword table is built from the previous draft's (removals, then additions that may replace an entry), sorted by name. It lists every keyword the draft's metaschema defines (draft 4 adds `$ref`, which JSON Reference defines) and where its value holds subschemas (`SubschemaPosition`: `Single`, `Array`, `Map`, `SingleOrArray`). The table is the single answer to "is this keyword known in this draft" (strict parsing) and "where are the subschemas" (the strict key walk and the draft-4 normalisation walk). 2019-09 and 2020-12 also carry their metaschema's `$vocabulary` list.

| | Draft 4 | Draft 6 | Draft 7 | 2019-09 | 2020-12 |
|---|---|---|---|---|---|
| id keyword | `id` | `$id` | `$id` | `$id` | `$id` |
| `ref_siblings` | `Ignored` | `Ignored` | `Ignored` | `Evaluated` | `Evaluated` |
| `integer_semantics` | `Lexical` (`1.0` is not an integer) | `Numeric` | `Numeric` | `Numeric` | `Numeric` |
| `format_mode` | `Assertion` | `Assertion` | `Assertion` | `Annotation` | `Annotation` |
| `exclusive_bounds` | `BooleanModifier` | `Numeric` | `Numeric` | `Numeric` | `Numeric` |
| vocabularies | — | — | — | metaschema's six (format `false`) | metaschema's seven (all `true`) |

  Drafts 00–03 have no table of their own and use draft 4's rules with a `DialectWarning::UnsupportedDraft`; draft 5 is draft 4 (editorial changes only).
- **Who reads each flag.** `format_mode` gates every format check in the validator (today `uuid`, behind the `uuid` feature). `integer_semantics` is read by the validator's `type: "integer"` check. `exclusive_bounds` is read at parse time: under `BooleanModifier` the draft-4 boolean form is normalised to the numeric one, like `id` (see *exclusiveMinimum*). `ref_siblings` decides whether an `$id` next to `$ref` changes the base URI (the registry; drafts 4–7 ignore it), and is read by the compiled validator in #8 for sibling keywords (#28). The id keyword drives draft-4 `id` normalisation and the registry's resource discovery. `active_vocabularies` (a `VocabularySet`) is `VocabularySet::standard()` (every vocabulary but `format-assertion`) for every draft; only a custom metaschema changes it, and the validator skips the `validation` and `applicator` keywords when their vocabulary is not in force.
- **Resolution order** (`resolve_dialect(&schema, &settings)`): `JsonSchemaSettings::spec_version` when set; otherwise the root's `$schema` when it is a recognised metaschema URI (`SpecVersion::from_schema_uri`); otherwise **Draft 2020-12**. The settings' per-flag overrides are then applied to that draft's defaults: `format_mode`, `integer_semantics` and `exclusive_bounds` have one (`ref_siblings` gets its override with its consumer in #8, so there is never a setting that does nothing). Settings whose values are modes are enums (`FormatMode`, `IntegerSemantics`, `ExclusiveBounds`), not booleans. Parsing chooses the draft for the document root; the [registry](#schema-registry-ref-resolution) gives every embedded resource (a subschema with `$id`) the dialect of its own `$schema`, or its parent resource's when it has none. When the parse settings fixed the dialect (a pinned `spec_version` or a per-flag override, i.e. `JsonSchema::dialect` is `Some`), that dialect applies to every resource of the document and embedded `$schema`s are not consulted.
- **Unknown `$schema`.** Permissive by default (`UnknownSchemaUri::Fallback`): the schema uses Draft 2020-12 and the result carries a `DialectWarning::UnknownSchemaUri`. `UnknownSchemaUri::Reject` turns that into `JsonSchemaParseError::UnknownSchemaUri`. A pinned `spec_version` ignores `$schema`, so it never warns or errors. **Custom metaschemas.** At parse time an unrecognised URI is "unknown" (no documents are known yet). The registry then looks it up like any document (given documents, then the retriever): when found, its `$vocabulary` decides the active vocabularies (`core` always; a known vocabulary is active whether marked `true` or `false`; an unknown one marked `true` is refused with `RegistryError::UnsupportedVocabulary`, which the spec requires; an unknown optional one is ignored), the `format-assertion` vocabulary makes `format_mode` `Assertion`, and the metaschema's own `$schema` (followed through further custom metaschemas until a standard one) decides the base draft. Only when it cannot be loaded does the fallback above apply (logged at `debug`). `UnknownSchemaUri::Reject` refuses at parse time, before any registry exists, so a custom metaschema needs `Fallback` (the default).
- **Where the dialect lives.** Parsing resolves the dialect once. `JsonSchema::dialect` (field) holds it only when the settings chose something other than what `$schema` alone selects (a pinned version or an override); `None` means "follow `$schema`". `JsonSchema::dialect()` (method) returns the effective dialect either way, so `JsonSchema::try_from(text)` followed by `validate` follows the document's draft with nothing to configure, and a hand-built `JsonSchema` follows its `$schema` too. The field is not serialized and is `None` on every subschema.
- **Warnings and logging.** Warnings are typed data (`ResolvedDialect::warnings`, from `resolve_dialect`), and the library also logs each at `debug` level through the `log` facade, as library code should. The CLI prints them to stderr (`warning: <path>: ...`) and exposes the settings as `--jss-spec-version`, `--jss-format-mode`, `--jss-integer-semantics`, `--jss-exclusive-bounds`, `--jss-unknown-schema-uri`.
- **Draft-4 normalisation at parse time.** Two draft-4 shapes are rewritten into the one the model holds, in one iterative walk over schema positions from the keyword table (so a property named `id` is untouched): when the dialect's id keyword is `id`, a string `id` becomes `$id` (an object that already has `$id` keeps both keys as written); under `exclusive_bounds: BooleanModifier`, a boolean `exclusiveMinimum`/`exclusiveMaximum` becomes the numeric bound (see *exclusiveMinimum*). Lenient parsing goes straight from text to the model and re-parses through a `serde_json::Value` only when needed: for a draft-4 document whose text can contain an `id` key (a literal `"id"` or any `\u` escape), and when the direct parse fails on text that can contain an `exclusiveM...` key and the dialect reads the boolean form (otherwise the direct parse's error, with its line and column, is returned). Strict parsing always goes through a `Value`, because the dialect must be known before unknown keys are rejected. So does a lenient parse under `IntegerSemantics::Lexical` whose text can contain a count keyword (see *minLength / maxLength*).
- **Parsing: one keyword table, one model.** The draft's keyword table is the only list of keywords; the `JsonSchema` struct's `Deserialize` (hand-written: one `Field` list generated by a macro, every other key into `extensions`) is the only list of what we *hold*, used by lenient and strict parsing alike (there is no second strict-mode struct, converter or `type` deserializer). **Lenient** (default) ignores keys the model does not hold. **Strict** (`disallow_unknown_fields(true)`) is that same parse after one iterative key walk over the `Value`: every schema object the table's subschema positions reach (including those under keywords the model does not hold, such as `not` or `if`) is checked, and the first key the draft does not define is `JsonSchemaParseError::UnknownField { key, path }`, with `path` a `JsonPointer` to the schema object (root is the empty pointer; displayed as `root`). Root keys are checked before subschemas, and each object's keys in map order, so a document always reports the same key. Keys in non-schema positions (property names, `enum`/`const`/`default`/`examples` values, the string arrays of `dependencies`) are data and are not checked. Strict is spec-exact: it accepts every keyword the draft defines, whether or not this library implements it, and rejects anything else (`$defs` in draft 7, `$id` in draft 4, `id` from draft 6, `additionalItems` in 2020-12). The walk runs before draft-4 normalisation, so a draft-4 document is held to its keys as written. `new_from_serde_value` deserializes from the borrowed `Value` and copies it only for draft-4 normalisation. Both modes keep every key the model does not hold in `JsonSchema::extensions` (see *Schema model*); strict mode has only rejected the keys its draft does not define by then.

## 1. Core / identification

### $schema

**Our implementation:** We parse, store, and serialize `$schema` (round-trip). It is stored as `schema: Option<String>` on [`JsonSchema`] (serde `rename = "$schema"`). The root's `$schema` selects the document's [`Dialect`] (see *Dialects*): when [`JsonSchemaSettings::spec_version`] is `None` (default), the draft comes from `$schema` via [`SpecVersion::from_schema_uri`]; absent means **Draft 2020-12**; an unrecognised URI falls back to Draft 2020-12 with a warning, or is rejected under `UnknownSchemaUri::Reject`. `resolve_dialect(schema, settings)` returns the effective dialect with its warnings; `JsonSchema::dialect()` returns a parsed schema's. A `$schema` on an embedded resource (a subschema with `$id`) switches the dialect for that resource, and a custom metaschema's `$vocabulary` is honoured (see *Dialects*). Codegen does not use it to alter generated Rust. **Reverse codegen:** The `ToJsonSchema` derive emits a default `$schema` of `https://json-schema.org/draft/2020-12/schema` on the root schema so emitted documents are self-describing. **SpecVersion API:** [`SpecVersion::schema_uri()`] returns the canonical meta-schema URI for each variant; [`SpecVersion::from_schema_uri(s)`] parses a `$schema` URI string and returns the corresponding variant (or `None` for unknown/empty). Legacy draft-04 URI `http://json-schema.org/schema#` is accepted and maps to Draft04.

**Spec version quirks:** Draft 4 deprecated `http://json-schema.org/schema#` ("latest version"); specific version URIs are required for clarity. All drafts define `$schema` as an optional string (URI). It declares the dialect/meta-schema; it is not a validation keyword on instance data. Older drafts (00–02) used hyper-schema URIs in meta-schema files; draft-03 onward use `schema#` or (2019-09, 2020-12) `https://json-schema.org/draft/YYYY-MM/schema`.

### $id

**Our implementation:** We parse, store, and serialize `$id` (round-trip). It is stored as `id: Option<String>` on [`JsonSchema`] (serde `rename = "$id"`). In a draft-4 document, `id` is normalised to `$id` at parse time (see *Dialects*); we always emit `$id`. Codegen does not use it for struct naming. **Dedupe:** **Full** dedupe mode includes `id` in the dedupe key (two otherwise-identical schemas with different `$id` produce two structs). **Functional** dedupe mode does **not** include `id` in the key (same shape with different `$id` yields one shared struct). Reverse codegen: `#[json_schema(id = "...")]` on a struct or enum sets the emitted schema's `$id` and makes the type its own schema resource (own `$defs`, `#` for self-reference, embedded once; see *Rust struct → JSON Schema*, *Schema resources*). `$id` declares a schema resource in the [registry](#schema-registry-ref-resolution): it is resolved against the enclosing resource's base URI (RFC 3986) and becomes the base for relative `$ref`s inside it. A draft 4–7 `$id` that is only a fragment (`"#foo"`) is a plain-name anchor, not a resource. In drafts 4–7 an `$id` next to `$ref` is ignored (`ref_siblings: Ignored`). An `$id` inside a non-schema location (`enum`, `const`, an unknown keyword) is not an identifier. Two schemas declaring the same resource URI are a `RegistryError::DuplicateId`.

**Spec version quirks:** In the JSON Schema specification, draft-04 and earlier use the keyword `id` (no dollar sign); draft 6 and later use `$id`. A draft-4 document's `id` is read into the model's `$id`; in draft 6 and later `id` is an unknown keyword (ignored when lenient, rejected when strict). Serialization always writes `$id`, so a draft-4 document round-trips with `$id`.

### $anchor

**Our implementation:** The registry records each `$anchor` in the resource it appears in, and a `$ref` whose fragment is not a JSON Pointer (`"#foo"`, `"other.json#foo"`) resolves to it. Two schemas in one resource with the same anchor are a `RegistryError::DuplicateAnchor`; an unknown anchor makes the `$ref` unresolvable. The value stays in `JsonSchema::extensions` (round-trips).

**Spec version quirks:** `$anchor` exists from 2019-09. Drafts 6 and 7 express the same thing as an `$id` that is only a fragment (`"$id": "#foo"`), draft 4 as `"id": "#foo"`; the registry treats those as anchors. A `$anchor` in a draft-7 document is an unknown keyword and is not an anchor.

### $dynamicAnchor

**Our implementation:** Recorded by the registry like `$anchor` (2020-12 defines that a `$dynamicAnchor` is also a plain anchor), and kept separately per resource (`dynamic_anchors`) for `$dynamicRef`.

**Spec version quirks:** 2020-12 only (2019-09 has the boolean `$recursiveAnchor`, which is not resolved).

### $dynamicRef

Not resolved dynamically. `$dynamicRef` and 2019-09's `$recursiveRef` stay in `JsonSchema::extensions` and are not evaluated (the suite's `dynamicRef.json` / `recursiveRef.json` cases are known failures). The registry already keeps each resource's dynamic anchors; dynamic scope belongs to the compiled validator (#8; research/synthesis.md §3, item 4).

**Spec version quirks:** 2020-12 `$dynamicRef` / `$dynamicAnchor`; 2019-09 `$recursiveRef` / `$recursiveAnchor`.

### $vocabulary

Not a schema-model field (it stays in `JsonSchema::extensions`). Each 2019-09 / 2020-12 [`Dialect`] lists its standard metaschema's `$vocabulary` (`Dialect::vocabularies`) and carries the set in force (`Dialect::active_vocabularies`). A custom metaschema named by `$schema` is loaded through the registry and its `$vocabulary` decides that set; a required vocabulary we do not know is refused (`RegistryError::UnsupportedVocabulary`). See *Dialects*, *Custom metaschemas*.

**Spec version quirks:** (placeholder or blank)

### $comment

**Our implementation:** We parse, store, and serialize `$comment` (round-trip). The validator ignores it (no validation effect). Codegen does not emit it as user-facing doc (reserved for `description`); we do not emit `//` line comments in generated Rust. Dedupe: **Functional** mode excludes `comment` from the key (same shape with different `$comment` yields one struct); **Full** mode includes it (different comments yield separate structs). Reverse codegen: `#[json_schema(comment = "...")]` on a struct or enum sets the emitted schema’s `$comment`.

**Spec version quirks:** `$comment` is defined in draft-07 and later (2019-09, 2020-12); draft-06 and earlier do not define it. We accept the key when present in older drafts (lenient) but only draft-07+ define the keyword.

**Competitor approaches:** Rust: BelfordZ stores `comment` in the schema model; Stranger6667/typify do not. Other languages: many (json-everything, Corvus, networknt, santhosh-tekuri, pwall567, etc.) parse and store with no validation/codegen emission; jsonschema2pojo emits as Javadoc (spec says “should not be used to communicate to users”); Blaze emits only in Exhaustive mode. We align with parse+store+round-trip and no user-facing emission. See research reports under `research/reports/`.

---

## 2. Reference / reuse

### $ref

The JSON Schema `$ref` keyword references another schema by URI. Full resolution in the spec supports remote documents, `$id`-relative resolution, anchors, and arbitrary JSON Pointer fragments.

**Our implementation:**

- **Resolution** goes through the [schema registry](#schema-registry-ref-resolution), shared by the validator and codegen. A `$ref` is resolved against the base URI of the resource it appears in (RFC 3986); the fragment is a JSON Pointer from the target resource's root (percent-decoded, then RFC 6901-unescaped; empty tokens are keys) or an anchor name. Every `$ref` in every reachable document is resolved once, when the registry is built; a dangling one is `RegistryError::UnresolvableRef`, and a pointer to a value that is not a schema (`#/properties`, `#/required`, `#/title`) is `RegistryError::RefTargetNotSchema`. A pointer may go anywhere a schema can be: typed fields, keywords the model keeps raw (`not`, `if`, `patternProperties`), unknown keywords (`#/x-defs/a`) and data keywords (`#/examples/0`); a raw target is parsed once into a schema the registry keeps.
- **Schema model:** `$ref` is stored and round-tripped as `ref_: Option<String>` on [`JsonSchema`] (serde `rename = "$ref"`).
- **Validator:** each work item carries its registry `Scope`. On popping a schema, the validator follows its `$ref` chain (`follow_refs`: enter the node's scope, resolve, repeat), detecting a chain that returns to the same `(scope, $ref)` as an `InvalidRef` (an in-place loop through `anyOf` / `allOf` is #36, fixed by #8). It then validates against the final target only: keywords next to `$ref` are not evaluated in any draft yet (#28, #8), even though 2019-09 and later require it.
- **Codegen (forward, JSON Schema → Rust):** codegen walks copies and merges of subschemas (`allOf` merging), so it cannot carry scopes; it resolves each `$ref` with `Registry::resolve_anywhere` (the root resource's resolution of that `$ref`, else the first resource where it appears). That is exact for every document whose `$ref`s mean the same in every resource; per-scope resolution in codegen comes with #9's TypeGraph walk. A target that is a `$defs` / `definitions` entry names its struct by the entry key (`Resolved::definition_name`). Failures are `CodeGenError::RefResolution { ref_str, reason }` (or `CodeGenError::Registry` for a registry error not about one `$ref`); there is no fallback to `serde_json::Value`.
- **Reverse codegen:** `ToJsonSchema` and the derive macro emit `$defs` and `$ref` for nested custom types. Shared types (e.g. two fields with the same struct type) are placed in `$defs` and referenced via `$ref` at use sites. Recursive types, including mutual recursion and longer cycles, terminate through the generator's pending set: a type being built returns its `$ref` immediately (see *Rust struct → JSON Schema*, *Generator context*).

**Spec version quirks:**

- **Siblings:** drafts 4, 6 and 7 ignore keywords next to `$ref` (including `$id`, so a sibling `$id` does not change the base URI; the registry follows this); 2019-09 and 2020-12 evaluate them. The validator ignores sibling assertions in every draft until #8 reads `ref_siblings` (#28).
- **Containers:** drafts 4–7 use `#/definitions/...`, 2019-09 and 2020-12 `#/$defs/...`. A JSON Pointer is followed as written, so either container works in every draft.
- **Dynamic references:** `$dynamicRef` (2020-12) and `$recursiveRef` (2019-09) are not resolved (see *$dynamicRef*).

### $defs

The JSON Schema `$defs` keyword (draft 2019-09, 2020-12) is a container for reusable subschemas on the root or any subschema. Older drafts use `definitions` with the same semantics.

**Our implementation:**

- **Schema model and parsing:**
  - The in-memory model stores both containers:
    - `defs: Option<BTreeMap<String, JsonSchema>>` with serde `rename = "$defs"`.
    - `definitions: Option<BTreeMap<String, JsonSchema>>` for legacy drafts.
  - Both maps use `BTreeMap` for **deterministic iteration order**.
  - Both containers are parsed in every draft. Strict parsing follows the draft's keyword table: `$defs` is an unknown keyword before 2019-09, while `definitions` stays known in every draft (2019-09 and 2020-12 keep it for compatibility). Subschemas in either map are checked for unknown keys.
  - Round-trip tests assert that schemas with `$defs` / `definitions` serialize and parse back to equal `JsonSchema` values, including empty maps vs absence.
- **Role in validation:** `$defs` and `definitions` are **containers only**. They have no direct validation semantics; they are only read when resolving `$ref`. The validator never iterates over all defs on its own.
- **Role in codegen (forward):**
  - Struct collection iterates over the root’s `$defs` and `definitions` maps in **BTreeMap order** and emits **one struct per definition key** when the subschema is object-like.
  - When a property (or item / combinator branch) schema is a `$ref` to one of these definitions, we:
    - Resolve to the target subschema via the shared resolver.
    - Use the **definition key** as the struct name for that target.
    - Reuse the same Rust type wherever that definition is referenced.
  - The integration scenario `defs_ref` covers CLI codegen, compilation, and runtime deserialization for a schema that uses `$defs` plus `$ref`.
- **Role in reverse codegen:**
  - The derive macro for structs emits `$defs` and `$ref` for nested custom types (including inside `Vec`, `Option`, `HashSet`, `Box`). Shared types (two or more fields with the same type) produce one entry in `$defs` and `$ref` at each use site. Recursion (self, mutual, any cycle length) is detected at run time by the generator's pending set; the recursive edge emits only `$ref`.
  - The derive emits a single root-level `$defs` map: every named type registers its body in the one generator, and a hand-written impl's own `$defs` are hoisted by `inline_subschema`, so schemas with multiple levels of nesting (e.g. Root → Outer → Inner) have `$defs: { Outer, Inner }` at root with no nested `$defs` inside subschemas. The exception is a schema resource (a type with `id`, or a hand-written schema with `$id`): it carries the `$defs` its own refs need (see *Rust struct → JSON Schema*, *Schema resources*).
  - Primitives and container types (`String`, `Vec<T>`, etc.) do not add to `$defs`; they are inlined, as is `uuid::Uuid` (a string+format scalar leaf, `uuid` feature) and any hand-written impl that keeps the default `json_schema_with`. Structs and enums with `#[derive(ToJsonSchema)]` that are referenced from other types appear in `$defs`.

**Spec version quirks:**

- **Draft-04, 06, 07:** Use `definitions` at the root (and subschemas) as the reusable-subschemas container. We parse and store it in `JsonSchema::definitions` and resolve fragments of the form `#/definitions/Name`.
- **Draft 2019-09, 2020-12:** Use `$defs` instead. We parse and store it in `JsonSchema::defs` and resolve `#/$defs/Name`.
- **Interoperability:** We do **not** normalize `definitions` to `$defs` or vice versa at parse time. Both maps are preserved for round-trip and draft compatibility. A `$ref` pointer names its container (`/$defs/...` or `/definitions/...`) and is followed as written, independent of `$schema` or `SpecVersion`.

---

## 3. Composition

### allOf

The JSON Schema `allOf` keyword is an array of schemas; an instance validates if it validates against **every** element. (Draft-03 and later; draft-00/01/02 do not define allOf in the core meta-schema we vendor.)

**Our implementation:**

- **Ingestion:** We store `allOf` as-is. The in-memory `JsonSchema` has `all_of: Option<Vec<JsonSchema>>`. No merging at parse time.
- **Validator:** When `all_of` is present and non-empty, we validate the instance against **each** subschema (push each `(subschema, instance, path)` onto the validation stack); we collect all errors (no fail-fast). No merge.
- **Codegen:** When a schema node has `all_of`, we **merge on-the-fly** (in codegen only) into a single effective schema for that node, then run existing struct collection and emission on that merged result. We emit a **single** Rust model per node. Merge rules: properties union (same key: recursive merge for nested object/array items; first non-None for leaf/constraint); required union (dedupe, stable order); `type` `object` when all subschemas are object-like; title/description first non-empty; other keywords first non-None. If merge fails (e.g. conflicting property types, non-object subschema), we return a specific [`CodeGenError`](json_schema_rs/src/code_gen/error.rs) variant (see Codegen errors for merge failures below).
- **Reverse codegen:** **Not supported.** We do not emit `allOf` from Rust types (no macro annotation). Round-trip is intentionally lossy unless a future design adds support (e.g. attributes or a different representation).

**Round-trip and reverse codegen:** Schema round-trip (parse → serialize) and reverse codegen do **not** preserve `allOf`; the design is intentionally lossy. We do not add round-trip tests that assert allOf is preserved.

**Codegen errors for merge failures:** When on-the-fly merging fails, we return distinct `CodeGenError` variants: `AllOfMergeEmpty`, `AllOfMergeNonObjectSubschema`, `AllOfMergeConflictingPropertyType`, `AllOfMergeConflictingNumericBounds`, `AllOfMergeConflictingEnum`, `AllOfMergeUnsupportedSubschema`. We do not silently fall back. Which conflicts are fatal vs resolved (e.g. first-wins for title/description) is documented in the merge implementation and in this section.

**Spec version quirks:** allOf appears in draft-03 and later; behavior is consistent across drafts we support (array of schemas; instance must validate against every element). Draft-00, 01, 02 do not define allOf in the core meta-schema.

### anyOf

The JSON Schema `anyOf` keyword is an array of schemas; an instance validates if it validates against **at least one** element. (Draft-03 and later; draft-00/01/02 do not define anyOf in the core meta-schema we vendor.)

**Our implementation:**

- **Ingestion:** We store `anyOf` as-is. The in-memory `JsonSchema` has `any_of: Option<Vec<JsonSchema>>`. No merging at parse time.
- **Validator:** When `any_of` is present, we treat the schema as an **at-least-one** choice:
  - If the array is empty, there is no subschema to match; we report a single `ValidationError::NoSubschemaMatched { instance_path, subschema_count: 0 }`.
  - Otherwise, we validate the instance against each subschema in turn using the same validator loop. If **any** subschema yields no errors, validation for this keyword succeeds and we do not emit an error. If **all** subschemas fail, we emit one `NoSubschemaMatched { instance_path, subschema_count }` error for this anyOf.
- **Codegen (forward, JSON Schema → Rust):** We treat `anyOf` as a **union**:
  - Every schema node with non-empty `any_of` produces a Rust **enum** with one variant per subschema. Each variant’s type is the type for that branch (struct name, primitive, or nested enum) after resolving `allOf` for that branch. We do not merge branches.
  - Root-level `anyOf` is supported: when the root schema has non-empty `any_of`, we generate a root enum (one variant per subschema) plus structs for each branch. Root is no longer required to be `type: "object"` with `properties` in this case.
  - Property-level `anyOf` is supported: when a property schema has non-empty `any_of`, the field type is the corresponding anyOf enum (or `Option<...>` when not required).
  - Struct collection traverses into every `any_of` branch, so structs from all branches are emitted even when only some branches are reachable from a given property.
  - Empty `any_of` is rejected in codegen with `CodeGenError::AnyOfEmpty` (and the batch wrapper reports a per-schema error).
  - **Open decision: serde representation of union enums.** The emitted enum carries no `#[serde(...)]` representation attribute, so serde's default **externally tagged** form applies: a newtype variant deserializes from `{"VariantName": <value>}`, not from the bare JSON value the schema describes. Which representation to emit (untagged, internally/adjacently tagged when a discriminator is detectable, or a per-schema decision) is undecided. Revisit when union codegen next changes; competitor approaches and a candidate design are in [research/synthesis.md](research/synthesis.md) section 14. Applies equally to `oneOf`.
- **Reverse codegen:** **Not supported.** We do not emit `anyOf` from Rust types today (no attribute for `anyOf`, and no special handling in the derive). Round-trip is intentionally lossy for `anyOf` (schema → Rust → schema) in the same way it is for `allOf`.

**Round-trip and reverse codegen:** Schema round-trip (parse → serialize) preserves `anyOf` when present, but reverse codegen does not attempt to reconstruct it from Rust types. See the reverse-codegen note below.

**Spec version quirks:**

- **Draft-00, 01, 02:** `anyOf` is not present in the core meta-schema we vendor.
- **Draft-03 and later (including 2020-12):** `anyOf` is defined as an array of schemas; an instance must validate against at least one subschema. The 2020-12 applicator vocabulary defines `schemaArray` with `minItems: 1` for `anyOf`, so the *meta-schema* forbids empty arrays, but schemas in the wild may still contain them. We accept empty arrays at parse time and treat them as “no subschema matches” in the validator and as an error (`AnyOfEmpty`) in codegen.

**Reverse codegen (not implemented):**

Reverse codegen for `anyOf` is intentionally **not** implemented yet. We do not emit `anyOf` from Rust types (no attribute; round-trip is lossy). To add it later, we expect two complementary approaches:

1. **Explicit types:** A field attribute such as `#[json_schema(any_of = (TypeA, TypeB))]` so derive builds a property schema with `any_of: Some(vec![TypeA::json_schema(), TypeB::json_schema(), ...])`.
2. **Enum inference:** Extend the enum derive to support enums with data and emit `anyOf` (one subschema per variant type); optionally allow a struct-field flag like `#[json_schema(any_of)]` to document enum-typed fields as `anyOf`.

Either approach requires macro parsing of the attribute/enum and a corresponding `JsonSchema::any_of` field in the emitted struct; the explicit attribute is self-contained per field, while enum inference reuses the enum’s shape.

### oneOf

The JSON Schema `oneOf` keyword is an array of schemas; an instance validates if it validates against **exactly one** element (not zero, not two or more). (Draft-03 and later; draft-00/01/02 do not define oneOf in the core meta-schema we vendor.)

**Our implementation:**

- **Ingestion:** We store `oneOf` as-is. The in-memory `JsonSchema` has `one_of: Option<Vec<JsonSchema>>`. No merging at parse time.
- **Validator:** When `one_of` is present, we treat the schema as an **exactly-one** choice:
  - If the array is empty, there is no subschema to match; we report a single `ValidationError::NoSubschemaMatched { instance_path, subschema_count: 0 }`.
  - Otherwise, we validate the instance against each subschema in turn. If **exactly one** subschema yields no errors, validation for this keyword succeeds. If **zero** subschemas pass, we emit one `NoSubschemaMatched { instance_path, subschema_count }`. If **two or more** subschemas pass, we emit one `MultipleSubschemasMatched { instance_path, subschema_count, match_count }`.
- **Codegen (forward, JSON Schema → Rust):** We treat `oneOf` as a **union** (same shape as anyOf):
  - Every schema node with non-empty `one_of` produces a Rust **enum** with one variant per subschema. Each variant's type is the type for that branch (struct name, primitive, or nested enum) after resolving `allOf` for that branch. We do not merge branches.
  - Root-level `oneOf` is supported: when the root schema has non-empty `one_of`, we generate a root enum (one variant per subschema) plus structs for each branch.
  - Property-level `oneOf` is supported: when a property schema has non-empty `one_of`, the field type is the corresponding oneOf enum (or `Option<...>` if not required).
  - Struct collection traverses into every `one_of` branch. Empty `one_of` is rejected in codegen with `CodeGenError::OneOfEmpty`.
  - **Open decision: serde representation of union enums.** Same open decision as `anyOf` (see the anyOf codegen notes): the enum is emitted without a representation attribute, so serde's externally tagged default applies.
- **Reverse codegen:** **Not supported.** We do not emit `oneOf` from Rust types (no attribute; round-trip is lossy). Same options as anyOf for future work (explicit `any_of`/`one_of` attribute or enum inference).

**Spec version quirks:**

- **Draft-00, 01, 02:** `oneOf` is not present in the core meta-schema we vendor.
- **Draft-03 and later (including 2020-12):** `oneOf` is defined as an array of schemas; an instance must validate against exactly one subschema. The 2020-12 applicator vocabulary defines `schemaArray` with `minItems: 1` for `oneOf` in the meta-schema; schemas in the wild may still contain empty arrays. We accept empty arrays at parse time and treat them as "no subschema matches" in the validator and as an error (`OneOfEmpty`) in codegen.

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

**Our implementation:** `type_: Option<TypeSet>` holds every listed type (see *Schema model: struct vs enum*); an unknown type name is a parse error. **Validator:** the instance must match one of the types (`TypeSet::matches`, `integer` per the dialect's `integer_semantics`); a single-type set reports `ExpectedObject` / `ExpectedString` / `ExpectedInteger` / `ExpectedNumber` / `ExpectedArray` / `ExpectedBoolean` / `ExpectedNull`, any other set (two or more types, or none) `ExpectedType { expected: TypeSet, got }`. **Codegen** builds the Rust type from `JsonSchema::primary_type()`: the set's one type besides `null`, so `["string", "null"]` (in either order) generates the same type as `"string"` (making it `Option` is #9's nullable handling), and a property whose set has two or more non-null types is omitted from the struct, as a property without `type` is (no single Rust type holds it; #9 decides its representation). `object`, `string`, `integer`, `number`, `boolean` and `array` drive codegen (integer emits the narrowest integer type for its bounds, number `f32`/`f64`; see *Numbers*). See schema model in `json_schema_rs/src/json_schema/json_schema.rs` and `type_set.rs`, Rust codegen in `json_schema_rs/src/code_gen/rust_backend.rs`, and validator in `json_schema_rs/src/validator/mod.rs`. For type `"string"` specifically, see **7. Strings** below. For type `"boolean"`, see **type: "boolean"** in **8. Numbers** below.

**Spec version quirks:**

- **Draft-00, 01, 02:** The `type` keyword may be a string or an array. In the meta-schema, array items may be a type string or a schema object (`$ref`). We accept type-name strings only; a schema object in the type array is a parse error.
- **Draft-03:** The `type` keyword may be a string or an array. When it is an array, elements may be **type strings or schema objects** (nested schemas), and `"any"` is a type name. Drafts 00–03 use draft 4's rules (*Dialects*), so a schema object in the type array and `"any"` are parse errors.
- **`integer`:** draft 4 defines it lexically ("a JSON number without a fraction or exponent part"), draft 6 and later by value ("any number with a zero fractional part"). The dialect records this (`integer_semantics`) and the validator applies it; see *type: "integer"*.
- **Draft-04 and later:** The type array is restricted to **type strings only** (no schema objects). Array must have at least one element; elements must be unique. Same primitive type names in all drafts (simpleTypes: array, boolean, integer, null, number, object, string). Draft-05 has no schema.json in our vendor (PDFs only); type behavior matches draft-04/06 for our purposes.

### const

The JSON Schema `const` keyword (draft-06 and later) requires the instance to be exactly equal to the schema's const value (JSON value equality). It is equivalent to `"enum": [value]` with one element. We support it in the validator, codegen, and reverse codegen.

**Our implementation:**

- **Validator:** When `const_value` is present, the instance must equal that value by JSON Schema equality (`json_equal`: numbers by mathematical value, so `-2.0` equals `-2`; objects regardless of key order; see *8. Numbers*); otherwise we push `ValidationError::NotConst` with instance path, expected (const) value, and actual (instance) value. `"const": null` is a present constraint (`Some(Value::Null)`), not an absent keyword. We check const before enum; when both are present, satisfying const is sufficient.
- **Codegen:** Only string const is supported for codegen. When a property (or array items) schema has `const_value` that is a string, we treat it as a single-value string enum and reuse the existing enum emission path (one Rust enum with one variant). Non-string const (number, boolean, null, object, array) falls back to the existing type for that node (e.g. String or the type implied by other keywords) so that generated code always compiles.
- **Reverse codegen:** When a Rust unit enum has exactly one variant, we emit `const_value: Some(value)` and `enum_values: None` (i.e. `"const": <value>` in JSON). The value comes from the variant name or `#[serde(rename)]`. Multi-variant unit enums continue to emit `enum_values`.
- **allOf merge:** When merging string subschemas, if both have `const_value`, they must be equal or we return `CodeGenError::AllOfMergeConflictingConst`.

**Spec version quirks:**

- **Draft-04 and earlier:** The `const` keyword does not exist. Strict parsing allows `const` in the schema (we do not reject it for older drafts); validation and codegen apply when the key is present.
- **Draft-06, 07, 2019-09, 2020-12:** `const` is defined: instance must be equal to the const value. Behavior is identical across these drafts.

### enum

JSON Schema `enum` is an array of allowed values. The instance validates if it is equal to one of the elements. We support it in codegen (as a property type), the validator, and reverse codegen.

**Our implementation:**

- **Codegen:** Only string enums are supported. When a property schema has `enum` with at least one value and all values are strings, we emit a Rust enum type (name from property key or title, same as struct naming). Non-string values or mixed types cause fallback to `String`. Variant naming: each value is mapped to PascalCase via `to_pascal_case`; if the result is not a valid Rust type identifier (e.g. starts with digit, keyword `Self`), we prefix with `E`. When multiple JSON values map to the same variant name (e.g. `"a"` and `"A"` both → `A`), we append `0`, `1`, `2` with no underscore (e.g. `A0`, `A1`) to **all** variants in that collision set, so variant names stay UpperCamelCase and do not trigger rustc's `non_camel_case_types` warning. Values are sorted alphabetically for determinism. Duplicate JSON values in the schema produce a single Rust variant. Enum types are deduplicated across properties and schemas: the same set of string values yields one Rust enum (canonical name from first occurrence). Enums are emitted before structs so struct fields can reference them. Root schema must still be `type: "object"` with `properties`; enum is supported only as a property type.
- **Validator:** If `enum_values` is present and non-empty, the instance must be equal to one of the values by JSON Schema equality (`json_equal`: `0.0` matches `0`, `[1.0]` matches `[1]`, but `0` never matches `false`). When both `type` and `enum` are present, we validate type first then enum membership. Error: `NotInEnum` with instance path.
- **Reverse codegen:** `JsonSchema` serializes `enum_values` when present. The `ToJsonSchema` derive supports Rust unit enums: variant name (or `#[serde(rename)]`) becomes the allowed values; we emit `"type": "string"` and the `enum` values.
- In tests, use `r#"..."#` when expected output contains `]"`.

**Spec version quirks:**

- **draft-00, 01, 02:** `enum` is an array; instance must be one of the values. Meta-schema: type "array", optional. No minItems or uniqueness requirement in meta-schema.
- **draft-03, 04:** Validation text says the array MUST have at least one element and elements MUST be unique. draft-04 meta-schema: `minItems: 1`.
- **draft-06, 07, 2019-09, 2020-12:** Validation text says the array SHOULD have at least one element and elements SHOULD be unique. Meta-schema: `items: true` (any JSON value). We accept empty enum arrays (no validation constraint from enum) and allow duplicates in the schema (we dedupe in codegen).

---

## 5. Objects

### type: "object"

When a schema has `"type": "object"`, the instance must be a JSON object (a mapping of string keys to values). This meaning is **identical across all JSON Schema drafts** (draft-00 through 2020-12). We use it in codegen (root and nested schemas must have `type: "object"` and `properties` to generate a struct) and in the validator (an object instance must match the `type` set; `required`, `properties` and `additionalProperties` apply to every object instance, with or without `type`). `"type": ["object", "null"]` accepts objects and `null`, and codegen treats it as `object` (see **type**).

**Spec version quirks:** None for the meaning of "object"; the only draft differences are in the format of the `type` keyword (string vs array, and draft-03 array may contain schema objects), documented under **type**.
### properties

We use `properties` to build structs: each property becomes a struct field. Property keys are sanitized for Rust (e.g. `-` → `_`). When the Rust field name differs from the JSON key, we emit `#[serde(rename = "...")]`. Object schemas are traversed recursively; each object with `properties` yields a Rust struct. See `json_schema/json_schema.rs` and `code_gen/rust_backend.rs`.

**Spec version quirks:** (placeholder or blank)

### required

The `required` array lists property names that are required at that object level. When absent, all properties are optional per JSON Schema spec. When `required: []`, all properties are optional. We emit required properties as `T` and optional as `Option<T>`. The per-property `optional` keyword may be parsed and explicitly **ignored** in codegen; required vs optional is determined only by the object-level `required` array (future-proofing). Field ordering: stable (e.g. BTreeMap yields alphabetical order by property key). File-based expected output must match generator output exactly (e.g. trailing newlines).

**Spec version quirks:** (placeholder or blank)

### additionalProperties

**Spec audit (local specs under `specs/`):** In draft 2019-09 and 2020-12, `additionalProperties` is in the applicator vocabulary and accepts a schema (including boolean schemas: `true`/`false`). In draft-04 through draft-07 it appears in the validation/applicator meta-schema with the same meaning: value may be **absent**, **boolean**, or a **schema object**. When **absent**, draft-04 and later treat it as **allow** (no constraint on properties outside `properties`). When **false**, no property name outside `properties` (and outside `patternProperties` when present) is allowed. When **true**, same as absent. When a **schema**, each additional property's **value** must validate against that schema. We do not implement `patternProperties` yet, so "additional" means any key not in `schema.properties`.

**Our implementation:**

- **Schema model:** `additional_properties: Option<Schema>` on `JsonSchema`, like every subschema position: absent or `null` → `None`, `true` / `false` → `Schema::Bool`, an object → `Schema::Object`. Serialization omits `None` and writes the boolean or the schema as given.
- **Validator:** For object instances, compute keys not in `schema.properties` (no patternProperties yet). `None` / `true`: no action. `false`: push `ValidationError::DisallowedAdditionalProperty { instance_path, property }` per additional key. A schema object: push (sub_schema, value, path) for each additional key for recursive validation.
- **Codegen:** `false` → `#[serde(deny_unknown_fields)]` on the struct. A schema object → extra field `#[serde(default)] pub additional: BTreeMap<String, T>` where T is the Rust type for sub (or `serde_json::Value` fallback for complex/unsupported sub). `true`/absent: no attribute, no extra field. **DedupeKey** includes `additional_properties` so structs differing only by this keyword are not deduped.
- **Reverse codegen:** Closed structs (no catch-all map field) emit `additionalProperties: false` in the derive. `BTreeMap<String, V>` implements `ToJsonSchema` and emits `type: "object"` with `additionalProperties: V::json_schema()`.
- **allOf merge:** When allOf merges object schemas, additionalProperties is not yet merged in a special way; the merged object schema takes the first/merged properties and required; additionalProperties behavior for merged allOf may be extended later.

**Spec version quirks:** Draft-03 and earlier may define or default `additionalProperties` differently; when supporting those drafts, confirm against local specs and add version-specific behavior if needed. For draft-04 through 2020-12 we implement: absent/true = allow, false = forbid, schema = validate each additional property value.

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

When a schema has `"type": "array"`, the instance must be a JSON array. This meaning is **identical across all JSON Schema drafts** (draft-00 through 2020-12). We use it in codegen (as a property type: array with `items` yields `Vec<T>` or `Option<Vec<T>>`), in the validator (an array instance must match the `type` set; `items` and the count bounds apply to every array instance), and in reverse codegen (`Vec<T>` emits `type: "array"` and `items: T::json_schema()`). Array is supported only as a **property type**; root schema must still be `type: "object"` with `properties`.

**Spec version quirks:** None for the meaning of type `"array"`. The only draft differences are in the **form** of the `type` keyword (string vs array), documented under **4. Type and value constraints → type**.

### items

The `items` keyword defines the schema that all array elements must validate against when it is a **single schema** (object). We support only the single-schema form; we do not support `items` as an array of schemas (tuple typing) or draft 2020-12 `prefixItems` in this implementation.

**Our implementation:** We parse and store `items` as `Option<Schema>` (a schema object or a boolean schema; `true` reads as `{}` in codegen, so it emits `Vec<serde_json::Value>`; `false` rejects every element in the validator). When present with `type: "array"`, codegen emits `Vec<T>` (or `Option<Vec<T>>`) where `T` is the Rust type for the item schema (string → `String`, integer → `i64`, number → `f64`, object with properties → struct name, string enum → enum name, array with items → `Vec<Inner>`). Unsupported item types (e.g. `null`, `boolean` only) emit `Vec<serde_json::Value>`. The validator, when `items` is present and the instance is an array, validates each array element against the item schema (iterative stack, no recursion). Reverse codegen: `Vec<T>::json_schema()` returns `type: "array"` and `items: Some(Schema::from(T::json_schema()))`. `serde_json::Value` implements `ToJsonSchema` as the empty schema `{}`, so generated code with a `Vec<serde_json::Value>` field (`"items": true` or an untyped item schema) compiles with the `ToJsonSchema` derive. We support minItems, maxItems, and uniqueItems (see their subsections). prefixItems, contains, unevaluatedItems, etc. are not implemented; see the TODO subsections below.

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

**Our implementation:** We parse and store `min_items` and `max_items` as `Option<u64>`, through the shared non-negative-integer parser (see *minLength / maxLength*): `2.0` is `2`. **Validator:** When the instance is an array (with or without `type`), we check length against `min_items` (if present) and `max_items` (if present); we push `ValidationError::TooFewItems` or `TooManyItems` and continue (no fail-fast). Length bounds apply to all arrays regardless of `uniqueItems` (i.e. both Vec and HashSet in codegen). **Codegen:** Emitted Rust type remains `Vec<T>` or `HashSet<T>` (and optional variants) per existing rules; when the schema has `min_items` or `max_items`, we emit `#[json_schema(min_items = N, max_items = M)]` on the generated field (only the keys that are present) so generated models round-trip and reverse codegen reproduces the schema. **DedupeKey** includes `min_items` and `max_items` so array/set schemas with different bounds are not deduplicated. **Reverse codegen:** `Vec<T>::json_schema()` and `HashSet<T>::json_schema()` set `min_items: None`, `max_items: None`; the derive supports `#[json_schema(min_items = N, max_items = M)]` on array and set fields (Vec, HashSet, Option<Vec>, Option<HashSet>) and overlays them on the property schema.

**Spec version quirks:** Draft-04 meta uses positiveInteger for `maxItems` and positiveIntegerDefault0 for `minItems`; 2019-09 and 2020-12 use nonNegativeInteger / nonNegativeIntegerDefault0. Validation semantics are identical; we use `u64` and accept 0 for both where the spec allows. Integer-valued decimals (`2.0`) follow the integer rule of the dialect, as for `minLength`: rejected in draft 4, accepted from draft 6.

### uniqueItems

When `uniqueItems` is `true`, all array elements must be unique (JSON structural equality). Default/absent = false. Meaning is the same across draft-02 through 2020-12.

**Our implementation:** We parse and store `unique_items` as `Option<bool>`. **Validator:** When `unique_items == Some(true)` and the instance is an array (whatever `type` says), we check that no two elements are equal by JSON Schema equality (`json_equal`: `[1, 1.0]` and `[{"a":1,"b":2},{"b":2,"a":1}]` have duplicates); if one is found we push `ValidationError::DuplicateArrayItems` (naming the later of the two equal elements) and continue. Arrays of up to 15 elements are compared pairwise; longer ones go through one hash-set pass keyed by `JsonEqKey`, whose hash (`json_hash`) agrees with `json_equal` (a whole number hashes as an integer, so `1` and `1.0` collide; object entries hash in key order). The threshold follows jsonschema-rs and `research/synthesis.md` §11. **Codegen:** When `unique_items == Some(true)` and the item type is hashable (string, integer, number, or string enum), we emit `HashSet<T>` or `Option<HashSet<T>>`; otherwise we emit `Vec<T>` and the validator enforces uniqueness. **Reverse codegen:** `Vec<T>::json_schema()` does not set `unique_items` (omit = false). `HashSet<T>::json_schema()` returns `type: "array"`, `items: T::json_schema()`, and `unique_items: Some(true)`.

**Spec version quirks:** None; the keyword is boolean with the same meaning (all elements unique when true) across draft-02 through 2020-12.

### unevaluatedItems

TODO.

**Spec version quirks:** (placeholder or blank)

---

## 7. Strings

### Strings (type: "string")

When a schema has `"type": "string"` (or `"type": ["string", ...]` with string first), the instance must be a JSON string. This meaning is **identical across all JSON Schema drafts** (draft-00 through 2020-12).

**Our implementation:** `"string"` may be the whole `type` or one entry of a type array (see **4. type**). Validation: when `type` is `"string"`, a non-string produces `ValidationError::ExpectedString`. String length constraints (`minLength`, `maxLength`) and **pattern** are applied when the instance is a string (see **minLength / maxLength** and **pattern** below). Codegen: properties with `type: "string"` emit Rust `String` or `Option<String>` (with `#[serde(rename = "...")]` when the field name differs from the JSON key; `#[json_schema(min_length = N, max_length = N)]` and/or `#[json_schema(pattern = "...")]` when constraints are present). See schema model `primary_type()` and parsing in `json_schema_rs/src/json_schema/json_schema.rs`, validator in `json_schema_rs/src/validator/mod.rs`, and Rust backend in `json_schema_rs/src/code_gen/rust_backend.rs`. Reverse codegen: Rust `String` emits `"type": "string"` with no length or pattern bounds. Field-level `#[json_schema(min_length = N, max_length = N)]` and `#[json_schema(pattern = "...")]` attributes on String fields wire constraints through the derive macro.

**Spec version quirks:** None for the **meaning** of type `"string"` (instance must be a string). The only draft differences are in the **form** of the `type` keyword (string vs array, and draft-03 array may contain schema objects), documented under **4. Type and value constraints → type**.

### minLength / maxLength

`minLength` and `maxLength` constrain the length of string instances. The length is counted as Unicode code points (`.chars().count()`), not UTF-8 bytes. Both keywords are string-only; they are ignored when the instance is not a string (the `ExpectedString` error covers that case). Both are inclusive: `minLength: 3` means the string must have at least 3 code points; `maxLength: 50` means at most 50.

**Our implementation:**
- **Schema model:** `JsonSchema.min_length: Option<u64>` and `JsonSchema.max_length: Option<u64>` (serialized as `"minLength"` / `"maxLength"`). Both `None` by default; omitted from serialized JSON when absent.
- **Parsing:** every count keyword the model holds (`minLength`, `maxLength`, `minItems`, `maxItems`) goes through one shared non-negative-integer parser: any JSON number with a zero fractional part below 2^64 is accepted (`2`, `2.0`, `2e0`, `-0.0` as `0`) and stored as `u64`, so it serializes back as an integer; a fraction (`2.5`), a negative (`-1`, `-2.0`) or a value of 2^64 or more is a parse error (`expected a non-negative integer, got <n>`, with line and column). `minProperties`, `maxProperties`, `minContains` and `maxContains` use the same parser when the model holds them.
- **Draft 4 (`IntegerSemantics::Lexical`):** a count keyword written with a fraction or exponent (`2.0`, `2e0`) is not an integer, so it is `JsonSchemaParseError::InvalidKeywordValue { keyword, path, value, expected }`, in lenient and strict mode alike. This covers all eight count keywords, including those the model does not hold, because it decides whether the schema is valid, not what we implement. The rule follows the dialect's `integer_semantics` flag (the one `type: "integer"` reads), so the `integer_semantics` setting overrides it either way. The model stores `2.0` as `2`, so the written form is checked on the `Value` in the same walk as strict parsing's unknown-key check; a lenient draft-4 parse takes the `Value` path only when its text can contain a count keyword.
- **Validator:** For every string instance (with or without `type`), `s.chars().count()` is compared against `min_length` and `max_length`. Violations emit `ValidationError::TooShort { min_length }` or `ValidationError::TooLong { max_length }`. These are only emitted when the instance IS a string; a non-string under `type: "string"` gets only `ExpectedString`.
- **Codegen (JSON Schema → Rust):** When a string property schema has `min_length` and/or `max_length`, the backend emits `#[json_schema(min_length = N)]`, `#[json_schema(max_length = N)]`, or `#[json_schema(min_length = N, max_length = N)]` before the field declaration. This is consistent with the `min_items`/`max_items` pattern for array fields.
- **Reverse codegen (Rust → JSON Schema):** `String::json_schema()` returns `"type": "string"` with no length constraints. Field-level `#[json_schema(min_length = N, max_length = N)]` on a `String` (or `Option<String>`) field feeds into `property_inserts` in the derive macro, producing a schema with the specified `minLength`/`maxLength`. The `.or(base.min_length)` / `.or(base.max_length)` merge means field attributes take precedence over base type defaults (which are `None`).
- **Macro (`json_schema_to_rust!`):** The macro generates Rust source with the `#[json_schema(...)]` attributes on string fields; when the generated code is compiled, the `ToJsonSchema` derive re-derives the schema (round-trip verified in the test suite).

**Spec version quirks:** Present from Draft 3 onward. Draft 6 and later define the value as a non-negative integer by value, so `2.0` is valid (the suite's "… with a decimal" cases); draft 4's metaschema types it `integer`, which under draft 4's lexical integer rule excludes `2.0`. Draft 4 core defines an integer as "a JSON number without a fraction or exponent part", so we reject `2.0` there and accept it from draft 6 (the suite has no draft-4 decimal case either way). In Draft 3 and 4, `minLength` defaults to 0 (no minimum) and `maxLength` has no default (unconstrained). The character-counting semantics (`chars`, i.e., Unicode code points / UCS-2 code units in earlier drafts) have been stable. In practice, counting `.chars()` (Rust Unicode scalar values) matches the spec's intent for all common text.

#### pattern

The `pattern` keyword is a string value interpreted as an **ECMA 262** (JavaScript) regular expression. The instance string is valid if the pattern matches **anywhere** in the string (partial match); authors often use `^...$` for full-string match. Pattern is string-only; it is ignored when the instance is not a string.

**Our implementation:**
- **Schema model:** `JsonSchema.pattern: Option<String>` (serialized as `"pattern"`). `None` by default; omitted from serialized JSON when absent.
- **Validator:** For every string instance, after the `min_length`/`max_length` checks, if `schema.pattern.is_some()`, we compile the pattern with the **regress** crate (ECMA 262–compliant) in Unicode mode (the `u` flag), so `\p{Letter}` is a property escape and `.` matches a whole code point, as the suite's `ecmascript-regex` tests expect. On compile failure we push `ValidationError::InvalidPatternInSchema`. On success we run the regex against the instance string; if it does not match (partial match is success), we push `ValidationError::PatternMismatch`. Pattern is only checked when the instance is a string; a non-string under `type: "string"` gets only `ExpectedString`.
- **Codegen (JSON Schema → Rust):** When a string property schema has `pattern`, the backend emits `#[json_schema(pattern = "...")]` (value escaped for Rust string literals). Same pattern as `min_length`/`max_length`.
- **Reverse codegen (Rust → JSON Schema):** `String::json_schema()` returns `"type": "string"` with no pattern. Field-level `#[json_schema(pattern = "...")]` on a `String` (or `Option<String>`) field feeds into `property_inserts`, producing a schema with the specified `pattern`.
- **allOf merge:** When merging two string subschemas, if both have `Some(pattern)` and the values differ, we return `CodeGenError::AllOfMergeConflictingPattern`; otherwise we take the one that is `Some` when the other is `None`.

**Spec version quirks:** Pattern is defined from draft-03 onward; semantics (ECMA 262, partial match) are consistent across drafts we support. No draft-specific settings required.

#### contentEncoding / contentMediaType / contentSchema

TODO.

**Spec version quirks:** (placeholder or blank)

---

## 8. Numbers (integer / number)

### Numbers

Every number is held and compared **exactly**; there is no epsilon anywhere (`json_schema_rs/src/number.rs`).

- **Storage.** `minimum`, `maximum`, `exclusiveMinimum`, `exclusiveMaximum` and `multipleOf` are `Option<serde_json::Number>` in the model, so a bound keeps the kind it was written as (`9007199254740993` stays that integer; `1.5` is an `f64`), and `const` / `enum` values are `serde_json::Value`s. A `Number` is a `u64`, an `i64` or an `f64`; the IRs (#8, #9) lower it to `Num = Int(i128) | Float(f64)`.
- **Comparison (`cmp_num`).** Integers compare as integers (`i128`, so `u64` above `i64::MAX` and negatives mix), two `f64`s as `f64`s. An integer `x` against an `f64` `f` compares `x` rounded to the nearest `f64` first: when that differs from `f`, rounding cannot have crossed `f` (the rounded value is the double nearest `x`), so the order stands; when it is equal, `f` is a whole number within ±2^64 and the two compare as integers. This is exact without big-number arithmetic (jsonschema-rs's `NumCmp` reaches the same answers with a `BigFraction` fallback; boon's `as_f64()` does not, see `research/synthesis.md` §11).
- **Equality (`json_equal`).** `const`, `enum` and `uniqueItems` use JSON Schema equality: numbers by `cmp_num` (`1 == 1.0`), strings and booleans by value, arrays element by element, objects by key set and values regardless of order, and never across kinds (`0 != false`). The walk is iterative. `json_hash` is consistent with it (equal values hash equally): a whole number below 2^127 hashes as that integer, any other number by its `f64` bits, object entries in key order.
- **Beyond 64 bits.** `serde_json` (without `arbitrary_precision`) parses an integer that does not fit `u64`/`i64` as the nearest `f64`, and a number beyond `f64` range (`1e400`) is a parse error. So 2^64 and 2^64 + 1 compare equal, and a bignum instance is an `f64`. Exact big numbers are opt-in through #39.
- **Validator.** The numeric keywords apply to every numeric instance, whatever `type` says (`{"minimum": 1.1}` rejects `0.6`), and ignore non-numbers. Each failure is its own error carrying the bound and the instance exactly as written: `BelowMinimum`, `AboveMaximum`, `NotAboveExclusiveMinimum`, `NotBelowExclusiveMaximum`, `NotMultipleOf` (`root: value 9007199254740991 is below minimum 9007199254740993`). A `type: "integer"` instance with a fraction reports both `ExpectedInteger` and any bound it breaks.

### type: "integer"

When a schema has `"type": "integer"` (or `"type": ["integer", ...]` with integer first), the instance must be a JSON number that is an integer. What counts as an integer differs between drafts (see the quirks below).

**Our implementation:** `"integer"` may be the whole `type` or one entry of a type array (see **4. type**). Validation: when `type` is `"integer"`, the instance must be a number accepted by `is_integer` under the dialect's `integer_semantics`; otherwise `ValidationError::ExpectedInteger`. `Numeric` (draft 6+) accepts every `u64` / `i64` and every `f64` with a zero fractional part (`1.0`, `1e3`, a bignum such as `12345678910111213141516171819202122232425`); `Lexical` (draft 4) accepts the numbers `serde_json` read as `u64` / `i64`, i.e. written without a fraction or exponent and within 64 bits (`18446744073709551615` is an integer; `1.0` is not). `JsonSchemaSettings::integer_semantics` (`--jss-integer-semantics lexical|numeric`) overrides the draft. Bounds and `multipleOf` apply as for any number (see **Numbers**). Codegen: properties with `type: "integer"` emit the narrowest Rust integer type that holds every integer the bounds allow, else `i64` (see **minimum**). Reverse codegen: `i8`, `u8`, `i16`, `u16`, `i32`, `u32`, `i64`, `u64` emit `"type": "integer"` with `minimum` and `maximum` set to the type's range, exactly (`u64` → `0`..`18446744073709551615`).

**Spec version quirks:**

- **Draft 4 (and the drafts before it, which use draft 4's rules):** "a JSON number without a fraction or exponent part", so `1.0` is not an integer. A bignum written without fraction or exponent is an integer by this rule, but `serde_json` has already read it as an `f64`, indistinguishable from `1e52`, so we reject it (draft 4's `optional/bignum.json` integer cases stay known failures until #39).
- **Draft 6 and later:** "any number with a zero fractional part" (the draft-06 release notes state the change), so `1.0` is an integer.

### type: "number"

When a schema has `"type": "number"` (or `"type": ["number", ...]` with number first), the instance must be a JSON number (integer or floating-point). This meaning is **identical across all JSON Schema drafts** (draft-00 through 2020-12).

**Our implementation:** `"number"` may be the whole `type` or one entry of a type array (see **4. type**). Validation: when `type` is `"number"`, the instance must be a JSON number; otherwise `ValidationError::ExpectedNumber`. Bounds and `multipleOf` apply as for any number (see **Numbers**). Codegen: properties with `type: "number"` emit `f32` when both sides are bounded within `f32::MIN..=f32::MAX`, else `f64` (see **minimum**). Reverse codegen: `f32` and `f64` emit `"type": "number"` with `minimum` and `maximum` set to the type's finite range.

**Spec version quirks:** None for the **meaning** of type `"number"` across drafts. The only draft differences are in the **form** of the `type` keyword (string vs array), documented under **4. Type and value constraints → type**.

### type: "boolean"

When a schema has `"type": "boolean"` (or `"type": ["boolean", ...]` with boolean first), the instance must be a JSON boolean (`true` or `false`). This meaning is **identical across all JSON Schema drafts** (draft-00 through 2020-12).

**Our implementation:** `"boolean"` may be the whole `type` or one entry of a type array (see **4. type**). Validation: when `type` is `"boolean"`, a non-boolean produces `ValidationError::ExpectedBoolean`. Codegen: properties with `type: "boolean"` emit Rust `bool` or `Option<bool>` (required vs optional). Array items with `type: "boolean"` emit `Vec<bool>` or `HashSet<bool>` when `uniqueItems: true`. Reverse codegen: `ToJsonSchema for bool` already emits `"type": "boolean"`. See schema model `primary_type()` in `json_schema_rs/src/json_schema/json_schema.rs`, validator in `json_schema_rs/src/validator/mod.rs`, and Rust backend in `json_schema_rs/src/code_gen/rust_backend.rs`.

**Spec version quirks:** None for the **meaning** of type `"boolean"` across drafts. The only draft differences are in the **form** of the `type` keyword (string vs array), documented under **4. Type and value constraints → type**.

### minimum

The value of `minimum` MUST be a number, representing an inclusive lower limit for a numeric instance: a number is valid only if it is greater than or equal to `minimum`.

**Our implementation:** Stored exactly as `Option<serde_json::Number>`. **Validator:** for any numeric instance, `cmp_num(instance, minimum) == Less` pushes `ValidationError::BelowMinimum` (`minimum: 9007199254740993` rejects `9007199254740992`). **Codegen:** the bounds choose the Rust type. For an integer schema the lowest allowed integer is `⌈minimum⌉` and `⌊exclusiveMinimum⌋ + 1` (the tighter one wins), the highest `⌊maximum⌋` and `⌈exclusiveMaximum⌉ - 1`; when both sides exist the type is the narrowest of `u8`, `u16`, `u32`, `u64` (range starting at zero or above) or `i8`, `i16`, `i32`, `i64` that holds the range, else `i64` (an unbounded side, an empty range, or a range no 64-bit type holds). For a number schema, `f32` when both sides are bounded within `f32::MIN..=f32::MAX`, else `f64`. The bounds themselves are not emitted as attributes (the type carries them; self-validating types are #14). **`allOf` merge:** each bound keeps the tighter value, compared exactly; a `minimum` above the `maximum` is `CodeGenError::AllOfMergeConflictingNumericBounds`. **Reverse codegen:** integer types emit their exact range; the derive's `#[json_schema(minimum = N)]` overrides it, with `N` an integer or float literal, optionally negated, kept exactly as written (`minimum = -9223372036854775808` is that integer, not a rounded float).

**Spec version quirks:** Inclusive in draft 4 and later. Draft 4 makes it exclusive when `exclusiveMinimum: true` (normalised at parse time; see **exclusiveMinimum**). Drafts 00–02 had `minimumCanEqual` (default true); we implement only draft 4's rules for them.

### maximum

The value of `maximum` MUST be a number, representing an inclusive upper limit for a numeric instance.

**Our implementation:** As **minimum**, mirrored: `cmp_num(instance, maximum) == Greater` pushes `ValidationError::AboveMaximum`; `#[json_schema(maximum = N)]` in the derive.

**Spec version quirks:** As **minimum**, with `exclusiveMaximum` and `maximumCanEqual`.

### exclusiveMinimum

A numeric instance must be strictly greater than the exclusive lower limit.

**Our implementation:** Stored exactly as `exclusive_minimum: Option<serde_json::Number>`, the draft-6+ numeric form. **Validator:** for any numeric instance, `cmp_num(instance, exclusive_minimum) != Greater` pushes `ValidationError::NotAboveExclusiveMinimum` (the boundary itself is invalid). It applies independently of `minimum`; both may be present. **Codegen:** an exclusive bound counts for type selection (`exclusiveMinimum: -1` with `exclusiveMaximum: 256` emits `u8`; see **minimum**). **Reverse codegen:** `#[json_schema(exclusive_minimum = N)]` in the derive.

**Spec version quirks:** Draft 4 (and earlier): a **boolean** that makes `minimum` exclusive, and "if `exclusiveMinimum` is present, `minimum` MUST also be present". Draft 6 and later: a **number** that is itself the bound. The dialect records this (`exclusive_bounds`), and parsing normalises the draft-4 form so the model holds one shape: `"minimum": m, "exclusiveMinimum": true` becomes `"exclusiveMinimum": m` with `minimum` removed; `"exclusiveMinimum": false`, or `true` with no `minimum` (which constrains nothing), is removed. Under `BooleanModifier` a numeric `exclusiveMinimum` is also accepted and kept as the bound it states (its meaning is unambiguous). Under `Numeric` (draft 6+) a boolean value is a type error at parse time (`JsonSchemaParseError::Serde`), like any keyword with the wrong type. `JsonSchemaSettings::exclusive_bounds` (`--jss-exclusive-bounds boolean-modifier|numeric`) overrides the draft. A normalised draft-4 schema serializes in the numeric form.

### exclusiveMaximum

A numeric instance must be strictly less than the exclusive upper limit.

**Our implementation:** As **exclusiveMinimum**, mirrored: `cmp_num(instance, exclusive_maximum) != Less` pushes `ValidationError::NotBelowExclusiveMaximum`; `#[json_schema(exclusive_maximum = N)]` in the derive.

**Spec version quirks:** As **exclusiveMinimum**, with `maximum`.

### multipleOf

A numeric instance is valid only if dividing it by `multipleOf` gives an integer. The value MUST be a number strictly greater than 0.

**Our implementation:** Stored exactly as `multiple_of: Option<serde_json::Number>`. **Validator:** for any numeric instance, `!is_multiple_of(instance, multiple_of)` pushes `ValidationError::NotMultipleOf`. The test is exact, never an epsilon:

- **Both integers:** `instance % multipleOf == 0` in `i128`.
- **Otherwise:** each number is read as the decimal it was written as: an integer as itself, an `f64` as the shortest decimal that round-trips to it (Rust's `{:e}` formatting), so `0.1` is `1 · 10^-1` rather than the binary value nearest it. That is what the schema author wrote and what the suite expects (`0.0075` is a multiple of `0.0001`; an exact rational of the binary values would say no). Then with `instance = vm · 10^ve`, `multipleOf = dm · 10^de`, `vm = 2^s · 5^t · u` and `dm = 2^p · 5^q · r` (`u`, `r` coprime to 10), the quotient is an integer exactly when `r` divides `u`, `p ≤ s + k` and `q ≤ t + k`, with `k = ve - de`. Every step fits `u128` / `i64`, so `1e308` against `0.123456789` (false) or `0.5` (true) needs no big-integer arithmetic.
- **Valid values.** Every draft requires `multipleOf` to be strictly greater than 0, so parsing rejects `0` and negative values (`JsonSchemaParseError::Serde`, "multipleOf must be strictly greater than 0, got 0"), in lenient and strict mode alike, as it rejects `"minLength": -1`. An instance of `0` is a multiple of everything. A hand-built model that bypasses parsing gets a defined answer anyway: only `0` is a multiple of `0`, and signs are ignored.

**Codegen:** `multipleOf` does not change the emitted type and is not emitted as an attribute (self-validating types are #14). An `allOf` merge keeps the first subschema's `multipleOf` when both have one (it does not affect the type; typify merges by LCM). **Reverse codegen:** `#[json_schema(multiple_of = N)]` in the derive.

**Spec version quirks:** Drafts 2 and 3 call it `divisibleBy` (not modelled; we use draft 4's rules for them); it too must be greater than 0. Draft 4 and later: `multipleOf`, same meaning, and the same valid values: draft 4's text says "MUST be strictly greater than 0" (its metaschema: `minimum: 0, exclusiveMinimum: true`), draft 6 through 2020-12 say "MUST be a number, strictly greater than 0" (metaschema: `exclusiveMinimum: 0`). Draft 4 says "the result of the division of the instance by this keyword's value is an integer"; 2020-12 "division by this keyword's value results in an integer". Neither says how to divide, so implementations that use `f64` division disagree on decimals (`optional/float-overflow.json` tests one such case); exact decimal arithmetic agrees with the suite everywhere.

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

**Our implementation:** We treat empty or whitespace-only `description` as absent (no doc lines emitted). Multi-line description: one `///` doc line per non-empty trimmed line; no blank doc lines. **Placement:** object schema `description` → struct doc; enum schema (string enum) `description` → enum doc; property schema `description` → field doc. **Dedupe:** Functional mode excludes `description` from the dedupe key (same shape with different description yields one struct); Full mode includes it (different descriptions yield separate structs). **Reverse codegen:** `///` doc comments on a struct or enum become the root `description`, and on a field the property's `description` (beside its `$ref` for named-type fields). Each `#[doc = ...]` value is trimmed, values are joined with `\n`, leading and trailing blank lines are dropped, and an empty result is no `description`. A doc value that is a macro rustc expands to a string (`#[doc = include_str!("README.md")]`, `concat!`) is evaluated: the derive emits the expression into the generated code rather than skipping or rejecting it, so no doc text is dropped. List-form `#[doc(...)]` attributes (`hidden`, `alias`) carry no text and are skipped. Container `#[json_schema(description = "...")]` takes precedence over container doc comments. Because forward codegen emits `description` as `///` docs, `description` round-trips through generated types.

**Spec version quirks:** (placeholder or blank)

### default

The `default` keyword is a meta-data/annotation keyword. It specifies a value that may be used when the instance is absent; it does **not** affect validation. Validators must not reject or accept instances based on `default`.

**Our implementation:** We store `default` as `default_value: Option<serde_json::Value>` on `JsonSchema` (serialized as `"default"`). JSON `null` is stored as `Some(Value::Null)`; absent key is `None`. **Validator:** Parse and store only; no validation logic. **Codegen (schema → Rust):** UseTypeDefault — when schema default equals the Rust type's default (e.g. null, false, 0, `""`, `[]`, `{}`), emit `#[serde(default)]`. Custom — otherwise emit a generated default function and `#[serde(default = "fn")]`. Optional + null: use `#[serde(default)]`. Emission order: enums first, then default functions, then structs. **Dedupe:** Both Functional and Full modes include `default_value` in the dedupe key (identical defaults required for dedupe). **Reverse codegen:** Field-level `#[json_schema(default = "...")]` (or literal) merges into property schema `default_value`. **Macro:** Derive parses `default = ...` and passes into property_inserts. Out of scope: object defaults, non-empty array defaults.

**Spec version quirks:** In all drafts we support (draft-00 through 2020-12), `default` is defined as meta-data/annotation only with no validation effect. Draft 2019-09 and 2020-12 place it in the meta-data vocabulary explicitly. No behavioral differences between drafts for validation or codegen purposes.

**Parse note:** An object with two or more properties each having a `default` key can hit a serde_json map deserialization quirk (EOF while parsing). Single-property default and top-level schema default parse reliably; integration scenario and tests use a single property with default.

### examples

The `examples` keyword is meta-data/annotation (draft-06+). Its value is an array of example JSON values; there are no restrictions on the values. It does **not** affect validation. Validators must not reject or accept instances based on `examples`.

**Our implementation:** We store `examples` as `Option<Vec<serde_json::Value>>` on `JsonSchema` (serialized as `"examples"`). **Validator:** Parse and store only; no validation logic. **Codegen (schema → Rust):** Codegen emits examples in doc comments for generated structs and string enums (each example as a line under `# Examples`); no const emission. **Dedupe:** Full mode includes `examples` in the dedupe key (same shape with different examples yields two structs); Functional mode excludes it (different examples yield one struct). **Reverse codegen:** Serializing a `JsonSchema` that has `examples` emits them; hand-written structs do not set examples unless a future attribute is added. **allOf merge:** When merging object schemas, if the target has no examples we copy from the other subschema.

**Spec version quirks:** `examples` appears in draft-06 and later only; draft-00 through draft-05 do not define it. Draft-07 adds: when multiple occurrences of `examples` apply to a single sub-instance, implementations MUST provide a flat array of all values. We do not merge examples from multiple subschemas (e.g. allOf); we only copy when the target has none.

### deprecated

The `deprecated` keyword is a meta-data/annotation keyword (2019-09 and 2020-12 meta-data vocabulary). Type: boolean; default: false. It indicates that the instance or property should not be used; it does **not** affect validation. Draft-07 does not define `deprecated`; it was introduced in 2019-09.

**Our implementation:** We store `deprecated` as `Option<bool>` in the schema model. Validation: annotation only—instances that satisfy the rest of the schema remain valid; we do not emit deprecation warnings. Codegen: when a property, struct, or enum schema has `deprecated: Some(true)`, we emit `#[deprecated]` (or `#[deprecated = "description"]` when description is present) on the corresponding Rust field or type. Dedupe: included in **Full** mode only (like `description`/`comment`); omitted from Functional. Reverse codegen: field-level `#[to_json_schema(deprecated = true)]` emits `"deprecated": true` in the property schema.

**Spec version quirks:** Draft-07 does not define `deprecated`. It appears in the meta-data vocabulary for draft 2019-09 and 2020-12. We accept and store it regardless of `$schema`; default settings target 2020-12.

### readOnly / writeOnly

TODO.

**Spec version quirks:** (placeholder or blank)

### format

The `format` annotation is stored on `JsonSchema` as `Option<String>`. Currently only `"uuid"` is
acted upon; other values are preserved through parse/serialize round-trips but have no effect on
validation or codegen. **Assertion vs annotation** follows the dialect's `format_mode`: drafts 4, 6
and 7 check `format` by default (their specs say an implementation SHOULD, with an option to turn
it off); 2019-09 and 2020-12 only record it (2020-12: assertion "MUST be disabled by default").
`JsonSchemaSettings::builder().format_mode(..)` overrides either way. In annotation mode the
validator runs no format check; codegen's type mapping (`uuid` → `Uuid`) does not depend on it.

**UUID feature (`uuid` feature flag)**

Enable with `json-schema-rs = { features = ["uuid"] }` (or `["dep:uuid"]` internally). Activating
the feature:

- **Validation:** when the dialect asserts formats, string instances whose schema has
  `format: "uuid"` are validated with `uuid::Uuid::parse_str` (the check is applied in every
  asserting draft, although `uuid` is defined from 2019-09). An `InvalidUuidFormat` error is emitted for strings that do not conform
  to the UUID format.
- **Codegen:** string properties with `format: "uuid"` emit `uuid::Uuid` (required) or
  `Option<uuid::Uuid>` (optional) instead of `String`/`Option<String>`. A `use uuid::Uuid;`
  statement is automatically prepended. Array item schemas with `format: "uuid"` likewise produce
  `Vec<Uuid>` etc.
- **Reverse codegen:** `uuid::Uuid` implements `ToJsonSchema`, returning
  `{"type":"string","format":"uuid"}`. `Option<Uuid>`, `Vec<Uuid>`, and `HashSet<Uuid>` are
  supported through the existing generic impls. In a derived struct, a
  `uuid::Uuid` field is an **inlined scalar leaf**: the property schema is
  `{"type":"string","format":"uuid"}` directly (for `Vec<Uuid>`, an array whose `items` is that
  leaf), like the built-in primitives, rather than a `$ref` into `$defs`. This follows from the
  impl keeping the default `json_schema_with`, not from the field's tokens, so a user type
  named `Uuid` with `#[derive(ToJsonSchema)]` is a named type with its own `$defs` entry.
- **Macro:** the `json_schema_to_rust!` proc-macro and `#[derive(ToJsonSchema)]` both respect the
  `uuid` feature in the macro crate; enable with
  `json-schema-rs-macro = { features = ["uuid"] }`.

**Spec version quirks:** drafts 4, 6 and 7: assertion by default. 2019-09: the format vocabulary is
declared `false` in the metaschema, so annotation by default. 2020-12: the default metaschema uses
`format-annotation`; assertion only on opt-in or with the `format-assertion` vocabulary (a custom
metaschema whose `$vocabulary` lists it; see *Dialects*, *Custom metaschemas*).
