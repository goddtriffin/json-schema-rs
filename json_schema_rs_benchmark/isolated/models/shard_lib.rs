//! One shard of the benchmark's models: the fixtures' generated Rust types that `model_gen`
//! assigned to this crate, each in its own module, and the tables of this build's features that
//! map a fixture id to a model's entry point (or to why it has none). The models are split over
//! several crates so cargo compiles them in parallel.
//!
//! * `reverse-codegen`: `JSONSCHEMARS` (our derive's root schema) and `SCHEMARS` (schemars').
//! * `generated-types`: `TYPES` (the root type's deserialization entry points, default codegen
//!   settings) and `TYPES_NO_NEWTYPES` (the same with `constrained_newtypes(false)`).

/// One fixture's model for our derive: the root type's `ToJsonSchema::json_schema`, or why there
/// is none.
#[cfg(feature = "reverse-codegen")]
pub type OurModel = Result<fn() -> json_schema_rs::ReverseCodeGenResult, &'static str>;

/// One fixture's model for schemars: `schema_for!` of the root type, or why there is none.
#[cfg(feature = "reverse-codegen")]
pub type SchemarsModel = Result<fn() -> schemars::Schema, &'static str>;

/// One fixture's generated types: the root type's entry points, or why there are none.
#[cfg(feature = "generated-types")]
pub type Types = Result<json_schema_rs_benchmark::harness::GeneratedTypes, &'static str>;

include!(concat!(env!("OUT_DIR"), "/models.rs"));
