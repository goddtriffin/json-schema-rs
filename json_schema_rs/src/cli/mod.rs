//! CLI entry and subcommand dispatch for jsonschemars.

mod generate;
mod utils;
mod validate;

use clap::{Arg, Command};
use std::path::PathBuf;

#[expect(clippy::too_many_lines)]
pub fn run() {
    let cmd = Command::new("jsonschemars")
        .about("JSON Schema tooling: generate Rust types, validate JSON")
        .subcommand(
            Command::new("generate")
                .about("Generate Rust from one or more JSON Schema files or directories")
                .arg(
                    Arg::new("lang")
                        .required(true)
                        .value_name("LANG")
                        .help("Target language (e.g. rust)"),
                )
                .arg(
                    Arg::new("output")
                        .short('o')
                        .long("output")
                        .value_name("DIR")
                        .required(true)
                        .help("Output directory; generated .rs files use sanitized paths and each dir has a mod.rs"),
                )
                .arg(
                    Arg::new("inputs")
                        .required(true)
                        .value_name("INPUT")
                        .num_args(1..)
                        .help("JSON Schema file(s), directory(ies) to search for .json, or \"-\" for stdin"),
                )
                .arg(
                    Arg::new("jss-disallow-unknown-fields")
                        .long("jss-disallow-unknown-fields")
                        .action(clap::ArgAction::SetTrue)
                        .help("JSON Schema Settings: reject schema definitions with unknown keys"),
                )
                .arg(
                    Arg::new("cgs-model-name-source")
                        .long("cgs-model-name-source")
                        .value_name("SOURCE")
                        .value_parser(["title-first", "property-key"])
                        .help("Codegen Settings: primary source for struct/type names (default: title-first)"),
                )
                .arg(
                    Arg::new("cgs-dedupe-mode")
                        .long("cgs-dedupe-mode")
                        .value_name("MODE")
                        .value_parser(["disabled", "functional", "full"])
                        .help("Codegen Settings: dedupe identical object schemas (default: full)"),
                ),
        )
        .subcommand(
            Command::new("validate")
                .about("Validate a JSON instance against a JSON Schema (one schema, one payload)")
                .arg(
                    Arg::new("schema")
                        .short('s')
                        .long("schema")
                        .value_name("FILE")
                        .required(true)
                        .help("Path to the JSON Schema file. Use \"-\" for stdin."),
                )
                .arg(
                    Arg::new("payload")
                        .short('p')
                        .long("payload")
                        .value_name("FILE")
                        .help("Path to the JSON payload to validate. If omitted, read from stdin."),
                )
                .arg(
                    Arg::new("jss-disallow-unknown-fields")
                        .long("jss-disallow-unknown-fields")
                        .action(clap::ArgAction::SetTrue)
                        .help("JSON Schema Settings: reject schema definitions with unknown keys"),
                ),
        );
    let matches = cmd.get_matches();

    let result = match matches.subcommand() {
        Some(("generate", gen_m)) => {
            let lang: &str = gen_m
                .get_one::<String>("lang")
                .map(String::as_str)
                .expect("required LANG");
            let output_dir: PathBuf = gen_m
                .get_one::<String>("output")
                .map(|s| PathBuf::from(s.as_str()))
                .expect("required --output");
            let inputs: Vec<String> = gen_m
                .get_many::<String>("inputs")
                .map(|it| it.map(String::from).collect())
                .unwrap_or_default();
            let jss_disallow_unknown_fields: bool = gen_m.get_flag("jss-disallow-unknown-fields");
            let cgs_model_name_source: Option<&str> = gen_m
                .get_one::<String>("cgs-model-name-source")
                .map(String::as_str);
            let cgs_dedupe_mode: Option<&str> = gen_m
                .get_one::<String>("cgs-dedupe-mode")
                .map(String::as_str);
            generate::run_generate(
                lang,
                &inputs,
                &output_dir,
                jss_disallow_unknown_fields,
                cgs_model_name_source,
                cgs_dedupe_mode,
            )
        }
        Some(("validate", val_m)) => {
            let schema: PathBuf = val_m
                .get_one::<String>("schema")
                .map(|s| PathBuf::from(s.as_str()))
                .expect("required --schema");
            let payload: Option<PathBuf> = val_m
                .get_one::<String>("payload")
                .map(|s| PathBuf::from(s.as_str()));
            let jss_disallow_unknown_fields: bool = val_m.get_flag("jss-disallow-unknown-fields");
            validate::run_validate(&schema, payload, jss_disallow_unknown_fields)
        }
        _ => {
            eprintln!("expected subcommand: generate or validate");
            std::process::exit(1);
        }
    };
    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
