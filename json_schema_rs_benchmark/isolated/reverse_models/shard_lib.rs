//! One shard of the reverse-codegen models: the fixtures' generated Rust types that
//! `reverse_model_gen` assigned to this crate, each in its own module, and the `JSONSCHEMARS` and
//! `SCHEMARS` tables that map a fixture id to the tool's root-schema function (or to why it has
//! none). The models are split over several crates so cargo compiles them in parallel.

/// One fixture's model for our derive: the root type's `ToJsonSchema::json_schema`, or why there
/// is none.
pub type OurModel = Result<fn() -> json_schema_rs::ReverseCodeGenResult, &'static str>;

/// One fixture's model for schemars: `schema_for!` of the root type, or why there is none.
pub type SchemarsModel = Result<fn() -> schemars::Schema, &'static str>;

include!(concat!(env!("OUT_DIR"), "/models.rs"));
