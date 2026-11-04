//! Triage of a failed `reverse-codegen` build of `isolated/`: reads cargo's
//! `--message-format=json` output on stdin and prints one `tool\tfixture\treason` row per model
//! a compiler error lies in, for the driver to append to the exclusion file and build again (see
//! `json_schema_rs_benchmark::reverse_models`).
//!
//! Exits non-zero, with the reason on stderr, when an error lies outside every model or no error
//! names a model: excluding models cannot fix that build.

use json_schema_rs_benchmark::reverse_models::{Exclusion, build_failures};
use std::io::Read as _;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut messages: String = String::new();
    if let Err(err) = std::io::stdin().read_to_string(&mut messages) {
        eprintln!("model_build_failures: cannot read stdin: {err}");
        return ExitCode::FAILURE;
    }
    match build_failures(&messages) {
        Ok(failures) => {
            for Exclusion {
                tool,
                fixture,
                reason,
            } in failures
            {
                println!("{tool}\t{fixture}\t{reason}");
            }
            ExitCode::SUCCESS
        }
        Err(reason) => {
            eprintln!("model_build_failures: {reason}");
            ExitCode::FAILURE
        }
    }
}
