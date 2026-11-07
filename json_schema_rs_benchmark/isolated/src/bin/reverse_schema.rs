//! Thin CLI over this crate's reverse-codegen tools, for the benchmark's CLI tier:
//! `reverse_schema <tool> <fixture id>` prints the JSON Schema that `tool` (`jsonschemars` or
//! `schemars`) generates for the root type of the fixture's model, which is compiled into this
//! binary (see `models/gen`), and exits 0. With no model for that tool and fixture, or an unknown
//! tool, it prints why to stderr and exits 1.
//!
//! No Rust reverse-codegen tool ships a command-line front end (both are derives), so this
//! binary is how they are timed end to end next to the other languages' tools: process start,
//! the derive's schema generation, serialization, and output.

use json_schema_rs_benchmark::harness::{Adapter, AdapterError};
use json_schema_rs_benchmark_isolated::IsolatedTool;
use std::io::Write as _;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [tool, fixture] = args.as_slice() else {
        eprintln!("usage: reverse_schema <jsonschemars|schemars> <fixture id>");
        return ExitCode::FAILURE;
    };
    let Some(tool) = IsolatedTool::ALL
        .iter()
        .copied()
        .find(|candidate: &IsolatedTool| candidate.id() == tool)
    else {
        eprintln!("reverse_schema: unknown tool `{tool}`");
        return ExitCode::FAILURE;
    };
    let schema: Result<String, AdapterError> =
        json_schema_rs_benchmark_isolated::reverse::model(tool, fixture)
            .map(IsolatedTool::reverse_codegen)
            .and_then(|reversed| reversed.to_json());
    match schema {
        Ok(text) => {
            let mut stdout: std::io::StdoutLock<'_> = std::io::stdout().lock();
            match writeln!(stdout, "{text}") {
                Ok(()) => ExitCode::SUCCESS,
                Err(err) => {
                    eprintln!("reverse_schema: cannot write the schema: {err}");
                    ExitCode::FAILURE
                }
            }
        }
        Err(err) => {
            eprintln!("reverse_schema: {err}");
            ExitCode::FAILURE
        }
    }
}
