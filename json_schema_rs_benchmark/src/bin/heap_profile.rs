//! In-process peak-heap profile of every (tool, fixture, operation) cell of the tools this crate
//! links, and the authoritative in-process compatibility check.
//!
//! Usage: `heap_profile --output <RAW_DIR>`; see [`harness::heap_profile_main`] for the files it
//! appends and how each cell is measured.
//!
//! Allocation is counted by `peak_alloc` installed as the global allocator (the workspace
//! forbids `unsafe_code`, which a hand-written `GlobalAlloc` would need).

use json_schema_rs_benchmark::adapters::Tool;
use json_schema_rs_benchmark::harness;
use peak_alloc::PeakAlloc;
use std::process::ExitCode;

#[global_allocator]
static PEAK: PeakAlloc = PeakAlloc;

fn main() -> ExitCode {
    harness::heap_profile_main::<Tool>(PEAK)
}
