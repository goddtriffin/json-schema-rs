//! CLI entry and subcommand dispatch for jsonschemars.

mod generate;
mod utils;
mod validate;

use clap::{Arg, ArgMatches, Command};
use json_schema_rs::{FormatMode, JsonSchemaSettings, SpecVersion, UnknownSchemaUri};
use std::path::PathBuf;

/// `--jss-spec-version` values, paired with the spec version each selects.
const SPEC_VERSIONS: [(&str, SpecVersion); 10] = [
    ("draft-00", SpecVersion::Draft00),
    ("draft-01", SpecVersion::Draft01),
    ("draft-02", SpecVersion::Draft02),
    ("draft-03", SpecVersion::Draft03),
    ("draft-04", SpecVersion::Draft04),
    ("draft-05", SpecVersion::Draft05),
    ("draft-06", SpecVersion::Draft06),
    ("draft-07", SpecVersion::Draft07),
    ("2019-09", SpecVersion::Draft201909),
    ("2020-12", SpecVersion::Draft202012),
];

/// JSON Schema Settings (`jss-`) arguments shared by every subcommand that reads a schema.
fn jss_args() -> [Arg; 4] {
    [
        Arg::new("jss-disallow-unknown-fields")
            .long("jss-disallow-unknown-fields")
            .action(clap::ArgAction::SetTrue)
            .help("JSON Schema Settings: reject schema definitions with unknown keys"),
        Arg::new("jss-spec-version")
            .long("jss-spec-version")
            .value_name("VERSION")
            .value_parser(SPEC_VERSIONS.map(|(name, _)| name))
            .help("JSON Schema Settings: use this draft's rules and ignore $schema (default: follow $schema, else 2020-12)"),
        Arg::new("jss-format-mode")
            .long("jss-format-mode")
            .value_name("MODE")
            .value_parser(["annotation", "assertion"])
            .help("JSON Schema Settings: check `format` (assertion) or only record it (annotation) (default: the draft's rule)"),
        Arg::new("jss-unknown-schema-uri")
            .long("jss-unknown-schema-uri")
            .value_name("POLICY")
            .value_parser(["fallback", "reject"])
            .help("JSON Schema Settings: an unrecognised $schema falls back to 2020-12 with a warning, or is rejected (default: fallback)"),
    ]
}

/// Builds [`JsonSchemaSettings`] from the arguments added by [`jss_args`].
fn jss_settings(matches: &ArgMatches) -> JsonSchemaSettings {
    let mut builder = JsonSchemaSettings::builder()
        .disallow_unknown_fields(matches.get_flag("jss-disallow-unknown-fields"));
    if let Some(name) = matches.get_one::<String>("jss-spec-version")
        && let Some((_, spec_version)) = SPEC_VERSIONS.iter().find(|(n, _)| n == name)
    {
        builder = builder.spec_version(*spec_version);
    }
    if let Some(mode) = matches.get_one::<String>("jss-format-mode") {
        builder = builder.format_mode(match mode.as_str() {
            "assertion" => FormatMode::Assertion,
            _ => FormatMode::Annotation,
        });
    }
    if let Some(policy) = matches.get_one::<String>("jss-unknown-schema-uri") {
        builder = builder.unknown_schema_uri(match policy.as_str() {
            "reject" => UnknownSchemaUri::Reject,
            _ => UnknownSchemaUri::Fallback,
        });
    }
    builder.build()
}

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
                .args(jss_args())
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
                .args(jss_args()),
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
            let schema_settings: JsonSchemaSettings = jss_settings(gen_m);
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
                &schema_settings,
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
            let schema_settings: JsonSchemaSettings = jss_settings(val_m);
            validate::run_validate(&schema, payload, &schema_settings)
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
