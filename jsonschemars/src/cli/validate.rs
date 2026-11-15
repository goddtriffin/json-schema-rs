//! `jsonschemars validate` subcommand: validate JSON instances against a JSON Schema.
//!
//! The schema file is read from its `file:` URI, so its relative `$ref`s reach the files beside
//! it. Without `--schema`, each payload's `$schema` property names its schema, resolved against
//! the payload's `file:` URI and retrieved like any other referenced document. Each `--resource`
//! document is registered under its own `file:` URI and its root `$id`; any other document a
//! `$ref` names is retrieved from inside `--file-root` (or over HTTP with `--allow-remote-refs`).
//! Files ending in `.yaml` or `.yml` are read as YAML.
//!
//! With `--output FORMAT`, the result is printed to stdout as JSON in that standard output
//! format (2020-12 core §12), annotations included for `basic`, `detailed` and `verbose`;
//! otherwise each error is one line on stderr. With several payloads, each error line starts with
//! the payload's path, and `--output` prints a JSON array of `{"instance", "output"}` records.
//!
//! Exit codes: `0` every payload conforms; `1` a payload does not conform;
//! `2` no verdict could be reached for a payload: a schema document or the payload could not be
//! read or parsed (one `error:` line per problem), or the schema cannot be used (one
//! `schema error:` line per problem).

use super::CliFailure;
use super::documents::{Document, no_verdict, read_documents, registry_builder};
use super::retrieval::{CliRetriever, RefRetrieval};
use super::utils::{read_payload_from_path, read_payload_from_reader};
use clap::ArgMatches;
#[cfg(feature = "linear-regex")]
use json_schema_rs::RegexEngine;
use json_schema_rs::{
    CompiledSchema, CompiledSchemaBuilder, JsonSchemaSettings, Output, OutputFormat, Registry,
    Schema, ValidationResult, file_uri,
};
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::io;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};

/// Exit code when the payload does not conform to the schema.
const EXIT_INVALID: i32 = 1;
/// The compile options from `--regex-engine`, `--regex-size-limit`, `--max-depth` and
/// `--max-errors`. The
/// first two need the `linear-regex` feature; without it, using them is a no-verdict failure.
#[cfg_attr(
    feature = "linear-regex",
    expect(
        clippy::unnecessary_wraps,
        reason = "fails only without the `linear-regex` feature"
    )
)]
pub(crate) fn compile_options(matches: &ArgMatches) -> Result<CompiledSchemaBuilder, CliFailure> {
    // Every format but `flag` reports the annotations of a valid payload.
    let annotations: bool = output_format(matches).is_some_and(|f| f != OutputFormat::Flag);
    let builder: CompiledSchemaBuilder = CompiledSchema::builder()
        .max_depth(matches.get_one::<usize>("max-depth").copied())
        .max_errors(matches.get_one::<NonZeroUsize>("max-errors").copied())
        .annotations(annotations);
    let engine: Option<&str> = matches
        .get_one::<String>("regex-engine")
        .map(String::as_str);
    let size_limit: Option<usize> = matches.get_one::<usize>("regex-size-limit").copied();
    #[cfg(feature = "linear-regex")]
    {
        let engine: RegexEngine = match engine {
            Some("linear") => RegexEngine::Linear,
            _ => RegexEngine::Ecma,
        };
        Ok(builder.regex_engine(engine).regex_size_limit(size_limit))
    }
    #[cfg(not(feature = "linear-regex"))]
    {
        if engine == Some("linear") || size_limit.is_some() {
            return Err(no_verdict(
                "--regex-engine linear and --regex-size-limit need jsonschemars built with the `linear-regex` feature"
                    .to_string(),
            ));
        }
        Ok(builder)
    }
}

/// How `validate` reports its verdicts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Report {
    /// One line per error on stderr, without the payload's values when `masked` (`--masked`);
    /// with `apply_defaults` (`--apply-defaults`), the payload with its defaults on stdout when
    /// it conforms.
    Lines { masked: bool, apply_defaults: bool },
    /// The output format `--output` names, on stdout, leaving out conforming payloads when
    /// `errors_only` (`--errors-only`).
    Output {
        format: OutputFormat,
        errors_only: bool,
    },
}

/// How `--output`, `--errors-only`, `--masked` and `--apply-defaults` ask the verdicts to be
/// reported.
pub(crate) fn report(matches: &ArgMatches) -> Report {
    match output_format(matches) {
        Some(format) => Report::Output {
            format,
            errors_only: matches.get_flag("errors-only"),
        },
        None => Report::Lines {
            masked: matches.get_flag("masked"),
            apply_defaults: matches.get_flag("apply-defaults"),
        },
    }
}

/// The format `--output` names, if any.
fn output_format(matches: &ArgMatches) -> Option<OutputFormat> {
    matches
        .get_one::<String>("output")
        .map(|format| match format.as_str() {
            "flag" => OutputFormat::Flag,
            "basic" => OutputFormat::Basic,
            "detailed" => OutputFormat::Detailed,
            _ => OutputFormat::Verbose,
        })
}

/// The files `validate` reads: the schema (`None`: each payload's `$schema` names it), the
/// payloads (none: one from stdin) and the resources.
pub(crate) struct Inputs<'a> {
    pub(crate) schema: Option<&'a Path>,
    pub(crate) payloads: &'a [PathBuf],
    pub(crate) resources: &'a [PathBuf],
}

/// A payload read from the command line.
struct Payload {
    /// The `file:` URI a relative `$schema` in it resolves against.
    base_uri: Option<String>,
    value: Value,
}

/// What became of one payload.
enum Verdict {
    /// No verdict: the `schema error:` lines that say why, and a summary.
    None {
        problems: Vec<String>,
        summary: String,
    },
    /// Validated, for [`Report::Lines`].
    Result(ValidationResult),
    /// Evaluated, for [`Report::Output`].
    Output(Output),
}

pub(crate) fn run_validate(
    inputs: &Inputs<'_>,
    schema_settings: &JsonSchemaSettings,
    retrieval: &RefRetrieval,
    compile: &CompiledSchemaBuilder,
    report: Report,
) -> Result<(), CliFailure> {
    if inputs.payloads.len() > 1
        && matches!(
            report,
            Report::Lines {
                apply_defaults: true,
                ..
            }
        )
    {
        return Err(no_verdict(
            "--apply-defaults validates one payload, and prints it with its defaults".to_string(),
        ));
    }
    let (schema, resources): (Option<Document>, Vec<Document>) =
        read_documents(inputs.schema, inputs.resources, schema_settings)?;
    let retriever: CliRetriever = CliRetriever::new(retrieval).map_err(no_verdict)?;
    let mut payloads: Vec<Result<Payload, String>> = if inputs.payloads.is_empty() {
        vec![read_payload_from_reader(io::stdin()).map(|value| Payload {
            base_uri: stdin_base_uri(),
            value,
        })]
    } else {
        inputs
            .payloads
            .iter()
            .map(|path| read_payload(path))
            .collect()
    };
    // Payloads that share a schema share its registry and compiled schema: (schema, its base URI)
    // → the payloads' indices.
    let mut groups: BTreeMap<(Option<String>, Option<String>), Vec<usize>> = BTreeMap::new();
    let mut verdicts: Vec<Option<Verdict>> = payloads.iter().map(|_| None).collect();
    for (index, payload) in payloads.iter().enumerate() {
        let key: Result<(Option<String>, Option<String>), String> = match payload {
            Err(e) => Err(e.clone()),
            Ok(_) if schema.is_some() => Ok((None, None)),
            Ok(payload) => {
                instance_schema(payload).map(|(reference, base)| (Some(reference), base))
            }
        };
        match key {
            Ok(key) => groups.entry(key).or_default().push(index),
            Err(summary) => {
                verdicts[index] = Some(Verdict::None {
                    problems: Vec::new(),
                    summary,
                });
            }
        }
    }
    let documents: Documents<'_> = Documents {
        schema: schema.as_ref(),
        resources: &resources,
        retriever: &retriever,
        settings: schema_settings,
        compile,
    };
    for ((reference, base_uri), indices) in groups {
        let compiled: Result<CompiledSchema, Vec<String>> =
            documents.compile(reference.as_deref(), base_uri.as_deref());
        for index in indices {
            let payload: &mut Payload = payloads[index]
                .as_mut()
                .expect("only payloads that were read are grouped");
            verdicts[index] = Some(judge(&compiled, payload, report));
        }
    }
    let named: Vec<(String, Option<Value>, Verdict)> = payloads
        .into_iter()
        .zip(verdicts)
        .zip(names(inputs.payloads))
        .map(|((payload, verdict), name)| {
            let value: Option<Value> = payload.ok().map(|p| p.value);
            (name, value, verdict.expect("every payload gets a verdict"))
        })
        .collect();
    print_verdicts(named, report)
}

/// What every payload's schema is compiled from.
struct Documents<'a> {
    /// The `--schema` document, if any.
    schema: Option<&'a Document>,
    resources: &'a [Document],
    retriever: &'a CliRetriever,
    settings: &'a JsonSchemaSettings,
    compile: &'a CompiledSchemaBuilder,
}

impl Documents<'_> {
    /// Compiles the `--schema` document or, without one, the schema `reference` (a payload's
    /// `$schema`) names, resolved against `base_uri`: one `schema error:` line per problem.
    fn compile(
        &self,
        reference: Option<&str>,
        base_uri: Option<&str>,
    ) -> Result<CompiledSchema, Vec<String>> {
        let wrapper: Schema;
        let (root, base_uri): (&Schema, Option<&str>) = match (self.schema, reference) {
            (Some(schema), _) => (&schema.schema, schema.uri.as_deref()),
            (None, Some(reference)) => {
                wrapper = Schema::new_from_serde_value(
                    &serde_json::json!({ "$ref": reference }),
                    self.settings,
                )
                .map_err(|e| schema_error_lines(std::iter::once(e)))?;
                (&wrapper, base_uri)
            }
            (None, None) => unreachable!("a payload without --schema is keyed by its $schema"),
        };
        let registry: Registry<'_> = registry_builder(
            base_uri,
            self.resources,
            self.retriever.clone(),
            self.settings,
        )
        .build(root)
        .map_err(|errors| schema_error_lines(errors.iter()))?;
        self.compile
            .build(&registry)
            .map_err(|errors| schema_error_lines(errors.iter()))
    }
}

/// Validates or evaluates one payload against its compiled schema, as `report` asks.
fn judge(
    compiled: &Result<CompiledSchema, Vec<String>>,
    payload: &mut Payload,
    report: Report,
) -> Verdict {
    match compiled {
        Err(problems) => Verdict::None {
            problems: problems.clone(),
            summary: format!("the schema cannot be used: {} problem(s)", problems.len()),
        },
        Ok(compiled) => match report {
            Report::Output { format, .. } => {
                Verdict::Output(compiled.evaluate(&payload.value, format))
            }
            Report::Lines {
                apply_defaults: true,
                ..
            } => Verdict::Result(compiled.validate_with_defaults(&mut payload.value)),
            Report::Lines { .. } => Verdict::Result(compiled.validate(&payload.value)),
        },
    }
}

/// Each payload's name in messages: its path, or `stdin` when there are none.
fn names(paths: &[PathBuf]) -> Vec<String> {
    if paths.is_empty() {
        vec!["stdin".to_string()]
    } else {
        paths.iter().map(|p| p.display().to_string()).collect()
    }
}

/// Reads a payload file, with the `file:` URI its `$schema` resolves against.
fn read_payload(path: &Path) -> Result<Payload, String> {
    let value: Value = read_payload_from_path(path)?;
    let base_uri: Option<String> = path.canonicalize().ok().and_then(|p| file_uri(&p));
    Ok(Payload { base_uri, value })
}

/// The URI a relative `$schema` in a payload on stdin resolves against: the current directory's.
fn stdin_base_uri() -> Option<String> {
    let directory: PathBuf = std::env::current_dir().ok()?;
    file_uri(&directory).map(|uri| format!("{}/", uri.trim_end_matches('/')))
}

/// The schema a payload names with its `$schema` property, and the base URI it resolves against
/// (`None` when it is an absolute URI, so payloads in different directories share it).
fn instance_schema(payload: &Payload) -> Result<(String, Option<String>), String> {
    let Some(reference) = payload.value.get("$schema").and_then(Value::as_str) else {
        return Err(
            "the payload has no `$schema` property naming its schema; pass one with --schema"
                .to_string(),
        );
    };
    let base_uri: Option<String> = if has_scheme(reference) {
        None
    } else {
        payload.base_uri.clone()
    };
    Ok((reference.to_string(), base_uri))
}

/// Whether a URI reference starts with a scheme (RFC 3986 §3.1), so it resolves to itself.
fn has_scheme(reference: &str) -> bool {
    reference.split_once(':').is_some_and(|(scheme, _)| {
        scheme.starts_with(|c: char| c.is_ascii_alphabetic())
            && scheme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    })
}

/// One `schema error: ` line per problem.
fn schema_error_lines(problems: impl Iterator<Item = impl std::fmt::Display>) -> Vec<String> {
    problems
        .map(|problem| format!("schema error: {problem}"))
        .collect()
}

/// One payload's output, when several are printed as an array.
#[derive(Serialize)]
struct Record {
    /// The payload's path.
    instance: String,
    output: Output,
}

/// Prints every payload's verdict in the order the payloads were given, and fails with `2` when
/// a payload got no verdict, else with `1` when one does not conform.
fn print_verdicts(
    verdicts: Vec<(String, Option<Value>, Verdict)>,
    report: Report,
) -> Result<(), CliFailure> {
    let several: bool = verdicts.len() > 1;
    let prefix = |name: &str| {
        if several {
            format!("{name}: ")
        } else {
            String::new()
        }
    };
    let mut no_verdict_count: usize = 0;
    let mut invalid: usize = 0;
    let mut error_count: usize = 0;
    let mut records: Vec<Record> = Vec::new();
    // The one payload's reason for no verdict, the failure's message.
    let mut last_summary: String = String::new();
    for (name, value, verdict) in verdicts {
        match verdict {
            Verdict::None { problems, summary } => {
                for problem in problems {
                    eprintln!("{}{problem}", prefix(&name));
                }
                if several {
                    eprintln!("error: {name}: {summary}");
                }
                last_summary = summary;
                no_verdict_count += 1;
            }
            Verdict::Result(Ok(())) => {
                if let (
                    Report::Lines {
                        apply_defaults: true,
                        ..
                    },
                    Some(value),
                ) = (report, value)
                {
                    let json: String = serde_json::to_string_pretty(&value)
                        .map_err(|e| no_verdict(format!("failed to serialize the payload: {e}")))?;
                    println!("{json}");
                }
            }
            Verdict::Result(Err(errors)) => {
                invalid += 1;
                error_count += errors.len();
                let masked: bool = matches!(report, Report::Lines { masked: true, .. });
                for e in &errors {
                    if masked {
                        eprintln!("{}{}", prefix(&name), e.masked());
                    } else {
                        eprintln!("{}{e}", prefix(&name));
                    }
                }
            }
            Verdict::Output(output) => {
                let valid: bool = output.is_valid();
                if !valid {
                    invalid += 1;
                }
                if valid
                    && matches!(
                        report,
                        Report::Output {
                            errors_only: true,
                            ..
                        }
                    )
                {
                    continue;
                }
                records.push(Record {
                    instance: name,
                    output,
                });
            }
        }
    }
    if matches!(report, Report::Output { .. }) {
        print_records(records, several)?;
    }
    if no_verdict_count > 0 {
        return Err(no_verdict(if several {
            format!("{no_verdict_count} payload(s) could not be validated")
        } else {
            last_summary
        }));
    }
    if invalid == 0 {
        return Ok(());
    }
    let message: String = match (report, several) {
        (Report::Output { .. }, false) => "validation failed".to_string(),
        (Report::Output { .. }, true) => format!("{invalid} payload(s) failed validation"),
        (Report::Lines { .. }, false) => format!("validation failed with {error_count} error(s)"),
        (Report::Lines { .. }, true) => {
            format!("{invalid} payload(s) failed validation with {error_count} error(s)")
        }
    };
    Err(CliFailure {
        message,
        exit_code: EXIT_INVALID,
    })
}

/// Prints the `--output` records: the one payload's output, or an array of every record.
fn print_records(mut records: Vec<Record>, several: bool) -> Result<(), CliFailure> {
    let json: Option<Result<String, serde_json::Error>> = if several {
        Some(serde_json::to_string_pretty(&records))
    } else {
        records
            .pop()
            .map(|record| serde_json::to_string_pretty(&record.output))
    };
    if let Some(json) = json {
        let json: String =
            json.map_err(|e| no_verdict(format!("failed to serialize the output: {e}")))?;
        println!("{json}");
    }
    Ok(())
}
