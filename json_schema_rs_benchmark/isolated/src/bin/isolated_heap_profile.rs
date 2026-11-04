//! In-process peak-heap profile and compatibility check of this build's tool, appended to the
//! main crate's captures. Usage and output: see `harness::heap_profile_main`.

use json_schema_rs_benchmark::harness;
use json_schema_rs_benchmark_isolated::IsolatedTool;
use peak_alloc::PeakAlloc;
use std::process::ExitCode;

#[global_allocator]
static PEAK: PeakAlloc = PeakAlloc;

fn main() -> ExitCode {
    harness::heap_profile_main::<IsolatedTool>(PEAK)
}
