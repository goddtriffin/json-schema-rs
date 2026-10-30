//! Lists every benchmark fixture for the CLI tier, using the same discovery as the in-process
//! tier (`BENCH_FIXTURE_ROOTS`, see `json_schema_rs_benchmark::fixtures`), so both tiers always
//! measure the same fixture set.
//!
//! Prints one tab-separated line per fixture, sorted by id:
//! `<fixture id>\t<fixture dir>\t<resource count>\t<unresolved refs>`. `<resource count>` is the
//! number of vendored documents under `resources/` (which the CLI tier lists itself, in the same
//! order), and `<unresolved refs>` the comma-separated list of `$ref`s naming a document the
//! fixture does not vendor (empty when every reference resolves; see `Fixture::unresolved_refs`).
//! The possibly empty field is last: `read` with a tab `IFS` merges consecutive tabs.

use json_schema_rs_benchmark::fixtures::{Fixture, discover_configured};
use std::process::ExitCode;

fn main() -> ExitCode {
    let fixtures: Vec<Fixture> = match discover_configured() {
        Ok(fixtures) => fixtures,
        Err(err) => {
            eprintln!("list_fixtures: {err}");
            return ExitCode::FAILURE;
        }
    };
    for fixture in &fixtures {
        let unresolved: Vec<String> = fixture.unresolved_refs();
        println!(
            "{}\t{}\t{}\t{}",
            fixture.id,
            fixture.dir.display(),
            fixture.resources.len(),
            unresolved.join(",")
        );
    }
    ExitCode::SUCCESS
}
