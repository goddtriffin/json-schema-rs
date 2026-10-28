//! Lists every benchmark fixture for the CLI tier, using the same discovery as the in-process
//! tier (`BENCH_FIXTURE_ROOTS`, see `json_schema_rs_benchmark::fixtures`), so both tiers always
//! measure the same fixture set.
//!
//! Prints one tab-separated line per fixture, sorted by id:
//! `<fixture id>\t<fixture dir>\t<external refs>`, where the last field is the comma-separated
//! list of the schema's external `$ref`s (empty when the schema is self-contained).

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
        let external: Vec<String> = fixture.external_refs();
        println!(
            "{}\t{}\t{}",
            fixture.id,
            fixture.dir.display(),
            external.join(",")
        );
    }
    ExitCode::SUCCESS
}
