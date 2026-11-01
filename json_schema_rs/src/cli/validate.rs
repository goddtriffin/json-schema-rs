//! `jsonschemars validate` subcommand: validate a JSON instance against a JSON Schema.
//!
//! Exit codes: `0` the payload conforms; `1` the payload does not conform (one line per error);
//! `2` no verdict could be reached: the schema or payload could not be read or parsed, or the
//! schema cannot be used (one `schema error:` line per problem).

use super::CliFailure;
use super::utils::{read_payload_from_path, read_payload_from_reader, read_schema_from_path};
use json_schema_rs::{JsonSchemaSettings, SchemaError, ValidateError, validate};
use std::io;
use std::path::PathBuf;

/// Exit code when the payload does not conform to the schema.
const EXIT_INVALID: i32 = 1;
/// Exit code when no verdict could be reached (unreadable input, unusable schema).
const EXIT_NO_VERDICT: i32 = 2;

/// A failure that reached no verdict about the payload.
fn no_verdict(message: String) -> CliFailure {
    CliFailure {
        message,
        exit_code: EXIT_NO_VERDICT,
    }
}

pub(crate) fn run_validate(
    schema_path: &PathBuf,
    payload_path: Option<PathBuf>,
    schema_settings: &JsonSchemaSettings,
) -> Result<(), CliFailure> {
    let schema = read_schema_from_path(schema_path, schema_settings).map_err(no_verdict)?;
    let instance: serde_json::Value = match payload_path {
        Some(p) => read_payload_from_path(&p),
        None => read_payload_from_reader(io::stdin()),
    }
    .map_err(no_verdict)?;
    match validate(&schema, &instance) {
        Ok(()) => Ok(()),
        Err(ValidateError::Instance(errors)) => {
            for e in &errors {
                eprintln!("{e}");
            }
            Err(CliFailure {
                message: format!("validation failed with {} error(s)", errors.len()),
                exit_code: EXIT_INVALID,
            })
        }
        Err(ValidateError::Schema(error)) => {
            let problems: Vec<String> = match &error {
                SchemaError::Registry(errors) => errors.iter().map(ToString::to_string).collect(),
                SchemaError::Compile(errors) => errors.iter().map(ToString::to_string).collect(),
            };
            for problem in &problems {
                eprintln!("schema error: {problem}");
            }
            Err(no_verdict(format!(
                "the schema cannot be used: {} problem(s)",
                problems.len()
            )))
        }
    }
}
