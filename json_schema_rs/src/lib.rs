//! Generate Rust structs from JSON Schema.
//!
//! Accepts a JSON Schema, parses it into an in-memory model, and emits Rust source
//! to a writer. Supported keywords and types are documented in the README.

pub mod codegen;
pub mod error;
pub mod json_pointer;
pub mod schema;
pub mod validation;

pub use codegen::generate_rust;
pub use error::Error;
pub use json_pointer::{JsonPointer, JsonPointerError};
pub use schema::Schema;
pub use validation::{ValidationError, ValidationResult, validate};
