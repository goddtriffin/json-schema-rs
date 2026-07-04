//! In-process criterion micro-benchmarks of OUR library's hot paths.
//!
//! These measure `json-schema-rs` only (competitors are timed end-to-end as CLIs under
//! `/usr/bin/time`). Splitting schema-compile from per-instance validate is only meaningful
//! in-process, which is exactly why these live here and feed our own regression gating.
//!
//! Four categories, each run across a couple of representative schema complexities:
//!   * `schema_compile`  — parse JSON text into a `JsonSchema` (schema-compile time).
//!   * `validate_valid`  — per-instance validation of ACCEPTED instances (schema/instance
//!     parsed outside the timed closure so parsing is not counted).
//!   * `validate_invalid`— per-instance validation of REJECTED instances (error-path).
//!   * `codegen`         — `generate_rust` emitting Rust from a schema.
//!
//! Schemas/instances are embedded as `const` literals so the benches are self-contained
//! and reliable even when the shared `../fixtures` tier is absent (it is produced by a
//! parallel agent). Reliability over coverage.
//!
//! Optional `dhat-heap` feature swaps in dhat's allocator for heap profiling; see the
//! `#[cfg(feature = "dhat-heap")]` block below. The default build never references dhat.

#[cfg(not(feature = "dhat-heap"))]
use criterion::criterion_main;
use criterion::{Criterion, black_box, criterion_group};
use json_schema_rs::{CodeGenSettings, JsonSchema, generate_rust, validate};

// Under the `dhat-heap` feature, route all allocations through dhat so we can profile
// heap usage of the hot paths. The default build (no feature) never mentions dhat.
#[cfg(feature = "dhat-heap")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

/// A small, flat object schema: the common case.
const SMALL_SCHEMA: &str = r#"{
    "type": "object",
    "properties": {
        "name": { "type": "string" },
        "age": { "type": "integer" }
    },
    "required": ["name"]
}"#;

/// Accepted by `SMALL_SCHEMA`.
const SMALL_VALID: &str = r#"{ "name": "ada", "age": 36 }"#;

/// Rejected by `SMALL_SCHEMA` (missing required "name", wrong type for "age").
const SMALL_INVALID: &str = r#"{ "age": "not-a-number" }"#;

/// A more complex, nested schema with arrays, constraints, and a sub-object.
const NESTED_SCHEMA: &str = r#"{
    "type": "object",
    "properties": {
        "id": { "type": "integer" },
        "name": { "type": "string", "minLength": 1, "maxLength": 64 },
        "tags": {
            "type": "array",
            "items": { "type": "string" },
            "uniqueItems": true,
            "minItems": 1
        },
        "address": {
            "type": "object",
            "properties": {
                "street": { "type": "string" },
                "city": { "type": "string" },
                "zip": { "type": "string", "minLength": 5, "maxLength": 5 }
            },
            "required": ["street", "city"]
        },
        "scores": {
            "type": "array",
            "items": { "type": "number" }
        }
    },
    "required": ["id", "name", "address"]
}"#;

/// Accepted by `NESTED_SCHEMA`.
const NESTED_VALID: &str = r#"{
    "id": 1,
    "name": "widget",
    "tags": ["a", "b", "c"],
    "address": { "street": "1 Main St", "city": "Anytown", "zip": "12345" },
    "scores": [1.0, 2.5, 3.0]
}"#;

/// Rejected by `NESTED_SCHEMA` (duplicate tags, too-long zip, missing "city").
const NESTED_INVALID: &str = r#"{
    "id": 1,
    "name": "widget",
    "tags": ["a", "a"],
    "address": { "street": "1 Main St", "zip": "123456789" },
    "scores": [1.0, 2.5]
}"#;

fn parse_schema(text: &str) -> JsonSchema {
    serde_json::from_str(text).expect("embedded schema must parse")
}

fn parse_instance(text: &str) -> serde_json::Value {
    serde_json::from_str(text).expect("embedded instance must parse")
}

/// schema-compile: parse JSON text into a `JsonSchema` (includes serde + model build).
fn bench_schema_compile(c: &mut Criterion) {
    let mut group = c.benchmark_group("schema_compile");
    group.bench_function("small", |b| {
        b.iter(|| {
            let schema: JsonSchema =
                serde_json::from_str(black_box(SMALL_SCHEMA)).expect("valid schema");
            black_box(schema);
        });
    });
    group.bench_function("nested", |b| {
        b.iter(|| {
            let schema: JsonSchema =
                serde_json::from_str(black_box(NESTED_SCHEMA)).expect("valid schema");
            black_box(schema);
        });
    });
    group.finish();
}

/// validate (valid): per-instance validation of ACCEPTED instances. Schema and instance
/// are parsed outside the timed closure so only validation is measured.
fn bench_validate_valid(c: &mut Criterion) {
    let small_schema = parse_schema(SMALL_SCHEMA);
    let small_instance = parse_instance(SMALL_VALID);
    let nested_schema = parse_schema(NESTED_SCHEMA);
    let nested_instance = parse_instance(NESTED_VALID);

    let mut group = c.benchmark_group("validate_valid");
    group.bench_function("small", |b| {
        b.iter(|| {
            let result = validate(black_box(&small_schema), black_box(&small_instance));
            black_box(result.is_ok());
        });
    });
    group.bench_function("nested", |b| {
        b.iter(|| {
            let result = validate(black_box(&nested_schema), black_box(&nested_instance));
            black_box(result.is_ok());
        });
    });
    group.finish();
}

/// validate (invalid): per-instance validation of REJECTED instances (error path). Schema
/// and instance are parsed outside the timed closure.
fn bench_validate_invalid(c: &mut Criterion) {
    let small_schema = parse_schema(SMALL_SCHEMA);
    let small_instance = parse_instance(SMALL_INVALID);
    let nested_schema = parse_schema(NESTED_SCHEMA);
    let nested_instance = parse_instance(NESTED_INVALID);

    let mut group = c.benchmark_group("validate_invalid");
    group.bench_function("small", |b| {
        b.iter(|| {
            let result = validate(black_box(&small_schema), black_box(&small_instance));
            black_box(result.is_err());
        });
    });
    group.bench_function("nested", |b| {
        b.iter(|| {
            let result = validate(black_box(&nested_schema), black_box(&nested_instance));
            black_box(result.is_err());
        });
    });
    group.finish();
}

/// codegen: emit Rust source from a schema via `generate_rust`. Schemas are parsed outside
/// the timed closure so only code generation is measured.
fn bench_codegen(c: &mut Criterion) {
    let small_schema = parse_schema(SMALL_SCHEMA);
    let nested_schema = parse_schema(NESTED_SCHEMA);
    let settings = CodeGenSettings::builder().build();

    let mut group = c.benchmark_group("codegen");
    group.bench_function("small", |b| {
        b.iter(|| {
            let output = generate_rust(
                black_box(std::slice::from_ref(&small_schema)),
                black_box(&settings),
            )
            .expect("small schema generates");
            black_box(output);
        });
    });
    group.bench_function("nested", |b| {
        b.iter(|| {
            let output = generate_rust(
                black_box(std::slice::from_ref(&nested_schema)),
                black_box(&settings),
            )
            .expect("nested schema generates");
            black_box(output);
        });
    });
    group.finish();
}

/// When `dhat-heap` is enabled, wrap the criterion run in a dhat `Profiler` so heap stats
/// are written on exit. Criterion owns `main` via `criterion_main!`, so we install the
/// profiler in a custom entry point and keep the guard alive for the whole run.
#[cfg(feature = "dhat-heap")]
fn main() {
    let _profiler = dhat::Profiler::new_heap();
    benches();
    Criterion::default().configure_from_args().final_summary();
}

#[cfg(not(feature = "dhat-heap"))]
criterion_main!(benches);

criterion_group!(
    benches,
    bench_schema_compile,
    bench_validate_valid,
    bench_validate_invalid,
    bench_codegen
);
