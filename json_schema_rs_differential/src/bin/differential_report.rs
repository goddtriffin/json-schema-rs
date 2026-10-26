//! Build the differential-testing report from Bowtie reports.
//!
//! Usage:
//!
//! ```text
//! differential_report --reports-dir <dir> --suite-dir <suite checkout> \
//!     --fixtures-dir <benchmark fixtures> --ours <implementation id> --out-dir <dir>
//! ```
//!
//! Reads `<reports-dir>/suite/<draft>.jsonl` and `<reports-dir>/suite/<draft>__optional.jsonl`
//! (the optional suite) for every draft and
//! `<reports-dir>/fixtures/<fixture id, / as __>__{valid,invalid}.jsonl` for every manifest
//! fixture, runs the in-process error-message comparison, and writes
//! `<out-dir>/results.json` and `<out-dir>/results.md`.

use json_schema_rs_differential::DifferentialReport;
use json_schema_rs_differential::load::{Config, run, write_outputs};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "usage: differential_report --reports-dir <dir> --suite-dir <dir> --fixtures-dir <dir> --ours <implementation id> --out-dir <dir>";

const FLAGS: [&str; 5] = [
    "--reports-dir",
    "--suite-dir",
    "--fixtures-dir",
    "--ours",
    "--out-dir",
];

fn parse_args(args: &[String]) -> Result<Config, String> {
    let mut values: BTreeMap<&str, String> = BTreeMap::new();
    let mut iter: std::slice::Iter<'_, String> = args.iter();
    while let Some(arg) = iter.next() {
        if arg == "--help" || arg == "-h" {
            return Err(USAGE.to_string());
        }
        let Some(flag) = FLAGS.iter().find(|f: &&&str| **f == arg.as_str()) else {
            return Err(format!("unknown argument {arg}\n{USAGE}"));
        };
        let value: &String = iter
            .next()
            .ok_or_else(|| format!("{flag} requires a value\n{USAGE}"))?;
        if values.insert(flag, value.clone()).is_some() {
            return Err(format!("{flag} given more than once\n{USAGE}"));
        }
    }
    let mut take = |flag: &str| -> Result<String, String> {
        values
            .remove(flag)
            .ok_or_else(|| format!("missing required {flag}\n{USAGE}"))
    };
    Ok(Config {
        reports_dir: PathBuf::from(take("--reports-dir")?),
        suite_dir: PathBuf::from(take("--suite-dir")?),
        fixtures_dir: PathBuf::from(take("--fixtures-dir")?),
        ours: take("--ours")?,
        out_dir: PathBuf::from(take("--out-dir")?),
    })
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let config: Config = match parse_args(&args) {
        Ok(config) => config,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };
    let (report, markdown): (DifferentialReport, String) = match run(&config) {
        Ok(output) => output,
        Err(e) => {
            eprintln!("differential_report: {e}");
            return ExitCode::FAILURE;
        }
    };
    if let Err(e) = write_outputs(&config.out_dir, &report, &markdown) {
        eprintln!("differential_report: {e}");
        return ExitCode::FAILURE;
    }
    eprintln!(
        "wrote {} and {}",
        config.out_dir.join("results.json").display(),
        config.out_dir.join("results.md").display()
    );
    ExitCode::SUCCESS
}
