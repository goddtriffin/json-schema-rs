//! `jsonschemars validate` subcommand: validate a JSON instance against a JSON Schema.

use super::utils::{read_payload_from_path, read_payload_from_reader, read_schema_from_path};
use json_schema_rs::{JsonSchemaSettings, validate};
use std::io;
use std::path::PathBuf;

pub(crate) fn run_validate(
    schema_path: &PathBuf,
    payload_path: Option<PathBuf>,
    jss_disallow_unknown_fields: bool,
) -> Result<(), String> {
    let schema_settings: JsonSchemaSettings = JsonSchemaSettings::builder()
        .disallow_unknown_fields(jss_disallow_unknown_fields)
        .build();
    let schema = read_schema_from_path(schema_path, &schema_settings)?;
    let instance: serde_json::Value = match payload_path {
        Some(p) => read_payload_from_path(&p)?,
        None => read_payload_from_reader(io::stdin())?,
    };
    match validate(&schema, &instance) {
        Ok(()) => Ok(()),
        Err(errors) => {
            for e in &errors {
                eprintln!("{e}");
            }
            Err(format!("validation failed with {} error(s)", errors.len()))
        }
    }
}
