//! In-process criterion timing of every tool this crate links, on every fixture.
//!
//! See [`harness::cross_tool_main`] for the benchmark ids, the timed work and the fixture set.

use json_schema_rs_benchmark::adapters::Tool;
use json_schema_rs_benchmark::harness;
use std::process::ExitCode;

fn main() -> ExitCode {
    harness::cross_tool_main::<Tool>()
}
