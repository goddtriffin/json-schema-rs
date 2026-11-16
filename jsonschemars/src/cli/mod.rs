//! CLI entry and subcommand dispatch for jsonschemars.

mod documents;
mod generate;
mod retrieval;
mod transform;
mod utils;
mod validate;

use clap::{Arg, ArgMatches, Command};
use json_schema_rs::{
    CompiledSchemaBuilder, ContentMode, ExclusiveBounds, FormatMode, IntegerSemantics,
    JsonSchemaSettings, MetaschemaCheck, RefSiblings, SpecVersion, UnknownFormats,
    UnknownSchemaUri,
};
use retrieval::RefRetrieval;
use std::path::PathBuf;
use transform::Transform;

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
fn jss_args() -> [Arg; 11] {
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
        Arg::new("jss-content-mode")
            .long("jss-content-mode")
            .value_name("MODE")
            .value_parser(["annotation", "assertion"])
            .help("JSON Schema Settings: check `contentEncoding` (base64) and `contentMediaType` (JSON) (assertion) or only record them (annotation) (default: the draft's rule, assertion only in draft 7)"),
        Arg::new("jss-unknown-formats")
            .long("jss-unknown-formats")
            .value_name("POLICY")
            .value_parser(["ignore", "reject"])
            .help("JSON Schema Settings: an asserted `format` naming a format the draft does not define is ignored, or makes the schema an error (reject) (default: ignore, except where the 2020-12 format-assertion vocabulary rejects it)"),
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
        Arg::new("jss-metaschema-check")
            .long("jss-metaschema-check")
            .value_name("CHECK")
            .value_parser(["skip", "validate"])
            .help("JSON Schema Settings: check each schema document against its draft's metaschema before using it (validate), or not (skip) (default: skip)"),
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
    if let Some(mode) = matches.get_one::<String>("jss-content-mode") {
        builder = builder.content_mode(match mode.as_str() {
            "assertion" => ContentMode::Assertion,
            _ => ContentMode::Annotation,
        });
    }
    if let Some(policy) = matches.get_one::<String>("jss-unknown-formats") {
        builder = builder.unknown_formats(match policy.as_str() {
            "reject" => UnknownFormats::Reject,
            _ => UnknownFormats::Ignore,
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
    if let Some(check) = matches.get_one::<String>("jss-metaschema-check") {
        builder = builder.metaschema_check(match check.as_str() {
            "validate" => MetaschemaCheck::Validate,
            _ => MetaschemaCheck::Skip,
        });
    }
    builder.build()
}

/// The `bundle` / `dereference` subcommand: a schema and its resources in, one document out.
fn transform_command(
    name: &'static str,
    about: &'static str,
    output_help: &'static str,
) -> Command {
    Command::new(name)
        .about(about)
        .arg(
            Arg::new("schema")
                .short('s')
                .long("schema")
                .value_name("FILE")
                .required_unless_present("schema_file")
                .conflicts_with("schema_file")
                .help("Path to the JSON Schema file. Use \"-\" for stdin."),
        )
        .arg(
            Arg::new("schema_file")
                .value_name("SCHEMA")
                .help("Path to the JSON Schema file, as an alternative to --schema"),
        )
        .arg(
            Arg::new("resource")
                .short('r')
                .long("resource")
                .value_name("FILE")
                .action(clap::ArgAction::Append)
                .help("A JSON Schema document the schema may reference, reached by its file: URI or its root $id. Repeatable."),
        )
        .arg(
            Arg::new("output")
                .short('o')
                .long("output")
                .value_name("FILE")
                .help(output_help),
        )
        .args(jss_args())
        .args(retrieval::retrieval_args())
}

#[expect(clippy::too_many_lines)]
pub fn run() {
    let cmd = Command::new("jsonschemars")
        .about("JSON Schema tooling: generate Rust types, validate JSON, bundle and dereference schemas")
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
                    Arg::new("cgs-map-type")
                        .long("cgs-map-type")
                        .value_name("TYPE")
                        .value_parser(["btree-map", "hash-map", "index-map"])
                        .help("Codegen Settings: Rust map type for objects' string-keyed entries (maps and a struct's additional properties) (default: btree-map; index-map needs the `indexmap` feature)"),
                )
                .arg(
                    Arg::new("cgs-flatten-single-entry-combinators")
                        .long("cgs-flatten-single-entry-combinators")
                        .action(clap::ArgAction::SetTrue)
                        .help("Codegen Settings: generate an anyOf / oneOf with exactly one entry as that entry's type instead of a one-variant enum (default: off)"),
                )
                .arg(
                    Arg::new("cgs-self-validating")
                        .long("cgs-self-validating")
                        .value_name("BOOL")
                        .value_parser(clap::value_parser!(bool))
                        .help("Codegen Settings: generated types validate themselves against their schema (validate(), TryFrom<serde_json::Value>, anyOf / oneOf variants chosen by validation); the generated code embeds the schema parts their validation reaches (default: true)"),
                )
                .arg(
                    Arg::new("cgs-constrained-newtypes")
                        .long("cgs-constrained-newtypes")
                        .value_name("BOOL")
                        .value_parser(clap::value_parser!(bool))
                        .help("Codegen Settings: generate a newtype for each string / integer / number / boolean whose schema asserts more than its Rust type holds (minLength, maxLength, pattern, bounds, multipleOf, ...); it validates on construction and deserialization (default: true)"),
                )
                .arg(
                    Arg::new("cgs-format-types")
                        .long("cgs-format-types")
                        .value_name("MODE")
                        .value_parser(["typed", "follow-dialect"])
                        .help("Codegen Settings: when a format with a Rust type gets it (uuid: uuid::Uuid with the `uuid` feature; date-time / date: chrono types with the `chrono` feature; ipv4 / ipv6: std::net): typed = always, even where 2019-09 / 2020-12 only annotate formats (a deliberate divergence); follow-dialect = only where the dialect asserts the format, else String (default: typed)"),
                )
                .arg(
                    Arg::new("cgs-derive")
                        .long("cgs-derive")
                        .value_name("PATH")
                        .action(clap::ArgAction::Append)
                        .help("Codegen Settings: a derive macro every generated struct, enum and union gets on top of its own (e.g. schemars::JsonSchema); repeatable"),
                )
                .arg(
                    Arg::new("cgs-attr")
                        .long("cgs-attr")
                        .value_name("ATTR")
                        .action(clap::ArgAction::Append)
                        .help("Codegen Settings: an attribute every generated struct, enum and union gets, written as the text inside #[...] (e.g. non_exhaustive); repeatable"),
                )
                .arg(
                    Arg::new("cgs-patch")
                        .long("cgs-patch")
                        .value_name("FILE")
                        .help("Codegen Settings: a JSON file of changes to single generated types, keyed by generated name: {\"Veg\": {\"rename\": \"Vegetable\", \"derives\": [\"Hash\"], \"attrs\": [\"non_exhaustive\"]}}"),
                )
                .arg(
                    Arg::new("cgs-convert")
                        .long("cgs-convert")
                        .value_name("TYPE=SCHEMA")
                        .action(clap::ArgAction::Append)
                        .help("Codegen Settings: generate TYPE wherever a subschema equals the JSON SCHEMA, ignoring annotations (e.g. 'rust_decimal::Decimal={\"type\":\"string\",\"format\":\"decimal\"}'); the first match wins; repeatable"),
                )
                .arg(
                    Arg::new("cgs-replace")
                        .long("cgs-replace")
                        .value_name("NAME=TYPE")
                        .action(clap::ArgAction::Append)
                        .help("Codegen Settings: use TYPE in place of the generated type NAME, which is not generated (e.g. Ipv4=my::Ipv4); repeatable"),
                )
                .arg(
                    Arg::new("cgs-crate")
                        .long("cgs-crate")
                        .value_name("[RENAME=]CRATE@VERSION")
                        .action(clap::ArgAction::Append)
                        .help("Codegen Settings: a crate the output depends on, whose types a subschema's x-rust-type extension may name; an extension whose version requirement VERSION meets uses the crate's type instead of generating one (e.g. my-types@1.2; * = any version, ! = never; RENAME= when the output knows the crate by another name); repeatable"),
                )
                .arg(
                    Arg::new("cgs-unknown-crates")
                        .long("cgs-unknown-crates")
                        .value_name("MODE")
                        .value_parser(["generate", "allow", "deny"])
                        .help("Codegen Settings: what an x-rust-type naming a crate not given by --cgs-crate does: generate = ignore the extension and generate the type; allow = use the extension's type; deny = fail (default: generate)"),
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
                        .help("Path to the JSON Schema file (YAML when it ends in .yaml or .yml). Use \"-\" for stdin. If omitted, each payload's `$schema` property names its schema, resolved against the payload's file"),
                )
                .arg(
                    Arg::new("payload")
                        .short('p')
                        .long("payload")
                        .value_name("FILE")
                        .action(clap::ArgAction::Append)
                        .help("Path to a JSON payload to validate (YAML when it ends in .yaml or .yml). Repeatable; each payload gets its own verdict. If omitted, one payload is read from stdin."),
                )
                .arg(
                    Arg::new("resource")
                        .short('r')
                        .long("resource")
                        .value_name("FILE")
                        .action(clap::ArgAction::Append)
                        .help("A JSON Schema document the schema may reference, reached by its file: URI or its root $id. Repeatable."),
                )
                .arg(
                    Arg::new("regex-engine")
                        .long("regex-engine")
                        .value_name("ENGINE")
                        .value_parser(["ecma", "linear"])
                        .help("The engine for pattern and patternProperties: ecma = ECMA-262 as the spec requires (backtracking); linear = linear-time matching for untrusted schemas, refusing backreferences and lookaround (linear needs the `linear-regex` feature; default: ecma)"),
                )
                .arg(
                    Arg::new("regex-size-limit")
                        .long("regex-size-limit")
                        .value_name("BYTES")
                        .value_parser(clap::value_parser!(usize))
                        .help("The most memory one pattern may compile to under --regex-engine linear; a larger pattern is a schema error (needs the `linear-regex` feature; default: no limit)"),
                )
                .arg(
                    Arg::new("max-depth")
                        .long("max-depth")
                        .value_name("LEVELS")
                        .value_parser(clap::value_parser!(usize))
                        .help("How deeply evaluation may nest subschemas before the instance fails; at most the built-in stack guard (default: the guard, 2048 levels in a release build)"),
                )
                .arg(
                    Arg::new("max-errors")
                        .long("max-errors")
                        .value_name("COUNT")
                        .value_parser(clap::value_parser!(std::num::NonZeroUsize))
                        .conflicts_with("output")
                        .help("Report at most this many errors, ending evaluation at the last one (default: every error)"),
                )
                .arg(
                    Arg::new("masked")
                        .long("masked")
                        .action(clap::ArgAction::SetTrue)
                        .conflicts_with("output")
                        .help("Print the errors without the payload's values (`/age: value is above maximum 10`), for logs of sensitive data"),
                )
                .arg(
                    Arg::new("apply-defaults")
                        .long("apply-defaults")
                        .action(clap::ArgAction::SetTrue)
                        .conflicts_with("output")
                        .help("Insert the `default` of every missing property the schema lists before validating, and print the payload with its defaults to stdout when it conforms"),
                )
                .arg(
                    Arg::new("output")
                        .long("output")
                        .value_name("FORMAT")
                        .value_parser(["flag", "basic", "detailed", "verbose"])
                        .help("Print the result to stdout as JSON in a standard output format (2020-12 core §12): flag = the verdict; basic = a flat list of errors, or of the annotations of a valid payload; detailed = the same nested by schema structure; verbose = every subschema evaluated. With several payloads, a JSON array of {\"instance\": FILE, \"output\": ...} (default: one line per error on stderr)"),
                )
                .arg(
                    Arg::new("errors-only")
                        .long("errors-only")
                        .action(clap::ArgAction::SetTrue)
                        .requires("output")
                        .help("With --output, leave out the output of every payload that conforms"),
                )
                .args(jss_args())
                .args(retrieval::retrieval_args()),
        )
        .subcommand(transform_command(
            "bundle",
            "Write one JSON Schema document that holds every document the schema references (2020-12 core §9.3)",
            "Write the bundle to this file (default: stdout)",
        ))
        .subcommand(transform_command(
            "dereference",
            "Write the schema with every reference replaced by its target, for tools that do not resolve references (a recursive reference becomes a JSON Pointer to its ancestor)",
            "Write the dereferenced schema to this file (default: stdout)",
        ))
        .subcommand(transform_command(
            "canonicalize",
            "Write the schema in one normal form (sorted keywords, deduplicated `type`, `enum` and `required`, no assertions that accept everything, `{}` as `true`), keeping its verdicts, annotations and references",
            "Write the canonical schema to this file (default: stdout)",
        ));
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
                .map_or_default(|it| it.map(String::from).collect());
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
            let schema: Option<PathBuf> = val_m
                .get_one::<String>("schema")
                .map(|s| PathBuf::from(s.as_str()));
            let payloads: Vec<PathBuf> = val_m
                .get_many::<String>("payload")
                .map_or_default(|it| it.map(PathBuf::from).collect());
            let resources: Vec<PathBuf> = val_m
                .get_many::<String>("resource")
                .map_or_default(|it| it.map(PathBuf::from).collect());
            let schema_settings: JsonSchemaSettings = jss_settings(val_m);
            let retrieval: RefRetrieval = RefRetrieval::from_matches(val_m);
            validate::compile_options(val_m).and_then(|compile: CompiledSchemaBuilder| {
                validate::run_validate(
                    &validate::Inputs {
                        schema: schema.as_deref(),
                        payloads: &payloads,
                        resources: &resources,
                    },
                    &schema_settings,
                    &retrieval,
                    &compile,
                    validate::report(val_m),
                )
            })
        }
        Some((name @ ("bundle" | "dereference" | "canonicalize"), transform_m)) => {
            let transform: Transform = match name {
                "bundle" => Transform::Bundle,
                "dereference" => Transform::Dereference,
                _ => Transform::Canonicalize,
            };
            let schema: PathBuf = transform_m
                .get_one::<String>("schema")
                .or_else(|| transform_m.get_one::<String>("schema_file"))
                .map(|s| PathBuf::from(s.as_str()))
                .expect("--schema or SCHEMA is required");
            let resources: Vec<PathBuf> = transform_m
                .get_many::<String>("resource")
                .map_or_default(|it| it.map(PathBuf::from).collect());
            let output: Option<PathBuf> = transform_m
                .get_one::<String>("output")
                .map(|s| PathBuf::from(s.as_str()));
            transform::run_transform(
                transform,
                &schema,
                &resources,
                output.as_deref(),
                &jss_settings(transform_m),
                &RefRetrieval::from_matches(transform_m),
            )
        }
        _ => {
            eprintln!(
                "expected subcommand: generate, validate, bundle, dereference or canonicalize"
            );
            std::process::exit(1);
        }
    };
    if let Err(failure) = result {
        eprintln!("error: {}", failure.message);
        std::process::exit(failure.exit_code);
    }
}
