//! In-process criterion timing of this build's tool on every fixture, into the same criterion
//! output as the main crate's `cross_tool` bench. See `harness::cross_tool_main`.

use json_schema_rs_benchmark::harness;
use json_schema_rs_benchmark_isolated::IsolatedTool;
use std::process::ExitCode;

fn main() -> ExitCode {
    harness::cross_tool_main::<IsolatedTool>()
}
