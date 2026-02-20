//! CLI entry point for jsonschemars: generate Rust from JSON Schema, validate JSON against a schema.

mod cli;

fn main() {
    cli::run();
}
