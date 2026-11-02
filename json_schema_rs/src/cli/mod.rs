//! CLI entry and subcommand dispatch for jsonschemars.

mod generate;
mod retrieval;
mod utils;
mod validate;

use clap::{Arg, ArgMatches, Command};
use json_schema_rs::{
    ExclusiveBounds, FormatMode, IntegerSemantics, JsonSchemaSettings, RefSiblings, SpecVersion,
    UnknownSchemaUri,
};
use retrieval::RefRetrieval;
use std::path::PathBuf;

/// Why a subcommand failed: the message for the final `error:` line and the process exit code.
#[derive(Debug, Clone, PartialEq, Eq)]
struct CliFailure {
    message: String,
    exit_code: i32,
}

/// Any failure reported as a message alone exits with `1`.
impl From<String> for CliFailure {
    fn from(message: String) -> Self {
        CliFailure {
            message,
            exit_code: 1,
        }
    }
}

/// `--jss-forced-spec-version` / `--jss-default-spec-version` values, paired with the spec version each selects.
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
fn jss_args() -> [Arg; 8] {
    [
        Arg::new("jss-disallow-unknown-fields")
            .long("jss-disallow-unknown-fields")
            .action(clap::ArgAction::SetTrue)
            .help(
                "JSON Schema Settings: reject schema definitions with keys the draft does not define as keywords",
            ),
        Arg::new("jss-forced-spec-version")
            .long("jss-forced-spec-version")
            .value_name("VERSION")
            .value_parser(SPEC_VERSIONS.map(|(name, _)| name))
            .help("JSON Schema Settings: process the whole document (embedded resources included) with this draft's rules, ignoring every $schema (default: follow $schema)"),
        Arg::new("jss-default-spec-version")
            .long("jss-default-spec-version")
            .value_name("VERSION")
            .value_parser(SPEC_VERSIONS.map(|(name, _)| name))
            .help("JSON Schema Settings: the draft for a document without $schema (or with an unloadable one) (default: 2020-12)"),
        Arg::new("jss-format-mode")
            .long("jss-format-mode")
            .value_name("MODE")
            .value_parser(["annotation", "assertion"])
            .help("JSON Schema Settings: check `format` (assertion) or only record it (annotation) (default: the draft's rule)"),
        Arg::new("jss-integer-semantics")
            .long("jss-integer-semantics")
            .value_name("RULE")
            .value_parser(["lexical", "numeric"])
            .help("JSON Schema Settings: `type: integer` accepts only numbers written without a fraction or exponent (lexical) or any number with a zero fractional part, such as 1.0 (numeric) (default: the draft's rule)"),
        Arg::new("jss-exclusive-bounds")
            .long("jss-exclusive-bounds")
            .value_name("SHAPE")
            .value_parser(["boolean-modifier", "numeric"])
            .help("JSON Schema Settings: read exclusiveMinimum/exclusiveMaximum as a boolean that makes minimum/maximum exclusive (boolean-modifier) or as the bound itself (numeric) (default: the draft's rule)"),
        Arg::new("jss-ref-siblings")
            .long("jss-ref-siblings")
            .value_name("RULE")
            .value_parser(["ignored", "evaluated"])
            .help("JSON Schema Settings: keywords next to $ref are ignored (ignored) or evaluated alongside it (evaluated) (default: the draft's rule)"),
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
    let spec_version = |flag: &str| {
        matches.get_one::<String>(flag).and_then(|name| {
            SPEC_VERSIONS
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, spec_version)| *spec_version)
        })
    };
    if let Some(spec_version) = spec_version("jss-forced-spec-version") {
        builder = builder.forced_spec_version(spec_version);
    }
    if let Some(spec_version) = spec_version("jss-default-spec-version") {
        builder = builder.default_spec_version(spec_version);
    }
    if let Some(mode) = matches.get_one::<String>("jss-format-mode") {
        builder = builder.format_mode(match mode.as_str() {
            "assertion" => FormatMode::Assertion,
            _ => FormatMode::Annotation,
        });
    }
    if let Some(rule) = matches.get_one::<String>("jss-integer-semantics") {
        builder = builder.integer_semantics(match rule.as_str() {
            "lexical" => IntegerSemantics::Lexical,
            _ => IntegerSemantics::Numeric,
        });
    }
    if let Some(shape) = matches.get_one::<String>("jss-exclusive-bounds") {
        builder = builder.exclusive_bounds(match shape.as_str() {
            "boolean-modifier" => ExclusiveBounds::BooleanModifier,
            _ => ExclusiveBounds::Numeric,
        });
    }
    if let Some(rule) = matches.get_one::<String>("jss-ref-siblings") {
        builder = builder.ref_siblings(match rule.as_str() {
            "ignored" => RefSiblings::Ignored,
            _ => RefSiblings::Evaluated,
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
                )
                .arg(
                    Arg::new("cgs-unknown-properties")
                        .long("cgs-unknown-properties")
                        .value_name("MODE")
                        .value_parser(["capture", "drop"])
                        .help("Codegen Settings: keep (capture) or ignore (drop) object keys the schema does not name, overriding additionalProperties (default: follow the schema)"),
                )
                .arg(
                    Arg::new("cgs-unique-items")
                        .long("cgs-unique-items")
                        .value_name("TYPE")
                        .value_parser(["vec", "hash-set", "btree-set", "index-set"])
                        .help("Codegen Settings: Rust collection for uniqueItems arrays; a set rejects a duplicate item when deserializing (default: vec; index-set needs the `indexmap` feature)"),
                )
                .arg(
                    Arg::new("cgs-flatten-single-entry-combinators")
                        .long("cgs-flatten-single-entry-combinators")
                        .action(clap::ArgAction::SetTrue)
                        .help("Codegen Settings: generate an anyOf / oneOf with exactly one entry as that entry's type instead of a one-variant enum (default: off)"),
                )
                .args(retrieval::retrieval_args()),
        )
        .subcommand(
            Command::new("validate")
                .about("Validate a JSON instance against a JSON Schema and the documents it references")
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
                    Arg::new("resource")
                        .short('r')
                        .long("resource")
                        .value_name("FILE")
                        .action(clap::ArgAction::Append)
                        .help("A JSON Schema document the schema may reference, reached by its file: URI or its root $id. Repeatable."),
                )
                .args(jss_args())
                .args(retrieval::retrieval_args()),
        );
    let matches = cmd.get_matches();

    let result: Result<(), CliFailure> = match matches.subcommand() {
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
            let retrieval: RefRetrieval = RefRetrieval::from_matches(gen_m);
            generate::code_gen_settings(gen_m)
                .and_then(|code_gen_settings| {
                    generate::run_generate(
                        lang,
                        &inputs,
                        &output_dir,
                        &schema_settings,
                        &code_gen_settings,
                        &retrieval,
                    )
                })
                .map_err(CliFailure::from)
        }
        Some(("validate", val_m)) => {
            let schema: PathBuf = val_m
                .get_one::<String>("schema")
                .map(|s| PathBuf::from(s.as_str()))
                .expect("required --schema");
            let payload: Option<PathBuf> = val_m
                .get_one::<String>("payload")
                .map(|s| PathBuf::from(s.as_str()));
            let resources: Vec<PathBuf> = val_m
                .get_many::<String>("resource")
                .map(|it| it.map(PathBuf::from).collect())
                .unwrap_or_default();
            let schema_settings: JsonSchemaSettings = jss_settings(val_m);
            let retrieval: RefRetrieval = RefRetrieval::from_matches(val_m);
            validate::run_validate(&schema, payload, &resources, &schema_settings, &retrieval)
        }
        _ => {
            eprintln!("expected subcommand: generate or validate");
            std::process::exit(1);
        }
    };
    if let Err(failure) = result {
        eprintln!("error: {}", failure.message);
        std::process::exit(failure.exit_code);
    }
}
