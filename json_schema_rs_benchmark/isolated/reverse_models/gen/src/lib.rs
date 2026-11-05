//! Generator of the reverse-codegen models, the build-dependency of every model shard crate
//! (`reverse_models/shard_<n>`; see `isolated/Cargo.toml`).
//!
//! The models: for every measurable fixture (and the CLI tier's startup baseline), the Rust types
//! our codegen generates for its schema (default settings, its resources reachable through an
//! in-memory retriever, as in the codegen benchmark), in one copy per tool:
//!
//! * `jsonschemars`: the generated source as is (it derives our `ToJsonSchema`).
//! * `schemars`: the same source with `schemars::JsonSchema` in place of our derive, and each
//!   `#[json_schema(...)]` constraint restated as the `#[schemars(...)]` attribute a schemars
//!   user would write (`length`, `range`, `regex`, `title`, `description`, `required`, an
//!   enum's literal values through a `transform`, and every other keyword through `extend`), so
//!   both derives describe the same schema. A one-of union keeps schemars' own `anyOf` for an
//!   untagged enum.
//!
//! **Shards.** The models are about 400K lines of generated Rust. In one crate they compile one
//! after another (rustc's front end is serial within a crate, and the benchmark profile has one
//! codegen unit), so they are spread over [`SHARDS`] crates that cargo compiles in parallel, each
//! with the same benchmark profile. Every shard's build script generates every fixture's model
//! (our codegen is fast) and assigns the (tool, fixture) models to shards the same way:
//! largest first, each to the shard with the fewest lines so far. A shard then writes only its
//! own models. The build then takes about as long as its slowest model: `generated/wide-object`'s
//! model alone takes about 7 minutes to optimize (#102), the models together 43 minutes in one
//! crate.
//!
//! Each copy is a module of its own (`$OUT_DIR/reverse_models/<tool>/<fixture>/model.rs`), so a
//! derive that cannot handle a model fails only that model. The driver turns such a build failure
//! into rows of the exclusion file (`BENCH_REVERSE_EXCLUDE`, see
//! `json_schema_rs_benchmark::reverse_models`), and an excluded model is left out, its table
//! entry holding the reason. A fixture whose codegen fails has no model for any tool.

use json_schema_rs::{CodeGenInput, CodeGenSettings, GenerateRustOutput, Schema, generate_rust};
use json_schema_rs_benchmark::fixtures::{self, Fixture};
use json_schema_rs_benchmark::reverse_models::{
    EXCLUDE_ENV, Exclusion, MODEL_FILE_NAME, MODELS_DIR_NAME, STARTUP_FIXTURE_ID, STARTUP_SCHEMA,
    model_dir_name, parse_exclusions,
};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use syn::punctuated::Punctuated;
use syn::visit_mut::VisitMut;

/// Number of model shard crates (`reverse_models/shard_0` .. `shard_7`). The largest model is
/// about a sixth of all model lines and the slowest one most of the build's time, so more shards
/// would not shorten it.
pub const SHARDS: usize = 8;

/// Tool ids; the first is ours.
const TOOLS: [&str; 2] = ["jsonschemars", "schemars"];

/// A model to build: the fixture id, and its schema text and resources.
struct Input {
    id: String,
    schema_text: String,
    resources: BTreeMap<String, String>,
}

/// One (tool, fixture) model before it is written: our codegen's output, or why there is none.
struct Planned<'a> {
    tool: &'static str,
    index: usize,
    input: &'a Input,
    generated: Result<&'a (String, String), &'a str>,
}

/// Build script of shard crate `CARGO_PKG_NAME` (`reverse-models-shard-<n>`): writes the shard's
/// models and `$OUT_DIR/models.rs`, its module declarations and lookup tables.
///
/// # Panics
///
/// Panics (failing the build) when the package name has no shard number, or the fixtures, the
/// exclusion file or the output directory cannot be read or written.
pub fn build_shard() {
    let package: String = std::env::var("CARGO_PKG_NAME").expect("cargo sets CARGO_PKG_NAME");
    let shard: usize = package
        .rsplit('-')
        .next()
        .and_then(|n: &str| n.parse().ok())
        .filter(|n: &usize| *n < SHARDS)
        .unwrap_or_else(|| panic!("{package} is not named reverse-models-shard-<0..{SHARDS}>"));
    let out_dir: PathBuf = PathBuf::from(std::env::var("OUT_DIR").expect("cargo sets OUT_DIR"));
    std::fs::write(out_dir.join("models.rs"), source(shard, &out_dir))
        .expect("cannot write models.rs");
}

/// The shard's generated module: its models' `mod` declarations and its `JSONSCHEMARS` and
/// `SCHEMARS` tables (fixture id to the tool's root-schema function, or why it has none).
fn source(shard: usize, out_dir: &Path) -> String {
    println!(
        "cargo::rerun-if-env-changed={}",
        fixtures::FIXTURE_ROOTS_ENV
    );
    println!("cargo::rerun-if-env-changed={EXCLUDE_ENV}");
    for root in fixtures::configured_roots() {
        if root.is_dir() {
            println!("cargo::rerun-if-changed={}", root.display());
        }
    }
    let excluded: BTreeMap<(String, String), String> = exclusions();
    let found: Vec<Fixture> =
        fixtures::discover_configured().unwrap_or_else(|e| panic!("fixture discovery failed: {e}"));
    let mut inputs: Vec<Input> = vec![Input {
        id: STARTUP_FIXTURE_ID.to_owned(),
        schema_text: STARTUP_SCHEMA.to_owned(),
        resources: BTreeMap::new(),
    }];
    for fixture in found {
        if fixture.unresolved_refs().is_empty() {
            inputs.push(Input {
                id: fixture.id,
                schema_text: fixture.schema_text,
                resources: fixture
                    .resources
                    .into_iter()
                    .map(|resource| (resource.uri, resource.text))
                    .collect(),
            });
        }
    }
    let generated: Vec<Result<(String, String), String>> = inputs.iter().map(generate).collect();
    let mut planned: Vec<Planned<'_>> = Vec::new();
    for (index, (input, generated)) in inputs.iter().zip(&generated).enumerate() {
        for tool in TOOLS {
            planned.push(Planned {
                tool,
                index,
                input,
                generated: generated.as_ref().map_err(String::as_str),
            });
        }
    }
    let assignment: Vec<usize> = assign(&planned);
    let models_dir: PathBuf = out_dir.join(MODELS_DIR_NAME);
    if models_dir.exists() {
        std::fs::remove_dir_all(&models_dir).expect("cannot clear the models directory");
    }
    let mut modules: String = String::new();
    let mut ours: String = String::new();
    let mut schemars: String = String::new();
    for (model, _shard) in planned
        .iter()
        .zip(&assignment)
        .filter(|(_model, assigned): &(&Planned<'_>, &usize)| **assigned == shard)
    {
        let entry: String = match model.generated {
            Ok((source, root)) => {
                model_entry(&models_dir, &mut modules, model, source, root, &excluded)
            }
            Err(reason) => format!("Err({:?})", format!("no model: {reason}")),
        };
        let table: &mut String = if model.tool == "schemars" {
            &mut schemars
        } else {
            &mut ours
        };
        let _ = writeln!(table, "    ({:?}, {entry}),", model.input.id);
    }
    format!(
        "{modules}\n/// Our derive's models in this shard, by fixture id.\n\
         pub static JSONSCHEMARS: &[(&str, OurModel)] = &[\n{ours}];\n\n\
         /// schemars' models in this shard, by fixture id.\n\
         pub static SCHEMARS: &[(&str, SchemarsModel)] = &[\n{schemars}];\n"
    )
}

/// The shard of each planned model: largest first (by lines of our generated source; a model
/// without source weighs nothing), each to the least-loaded shard, ties to the lowest. The
/// order depends only on the fixtures, so every shard's build script computes the same
/// assignment.
fn assign(planned: &[Planned<'_>]) -> Vec<usize> {
    let weight = |model: &Planned<'_>| -> usize {
        model
            .generated
            .map_or(0, |(source, _root): &(String, String)| {
                source.lines().count()
            })
    };
    let mut order: Vec<usize> = (0..planned.len()).collect();
    order.sort_by(|a: &usize, b: &usize| {
        weight(&planned[*b])
            .cmp(&weight(&planned[*a]))
            .then_with(|| planned[*a].input.id.cmp(&planned[*b].input.id))
            .then_with(|| planned[*a].tool.cmp(planned[*b].tool))
    });
    let mut load: [usize; SHARDS] = [0; SHARDS];
    let mut assignment: Vec<usize> = vec![0; planned.len()];
    for model in order {
        let shard: usize = (0..SHARDS)
            .min_by_key(|shard: &usize| (load[*shard], *shard))
            .unwrap_or(0);
        load[shard] += weight(&planned[model]);
        assignment[model] = shard;
    }
    assignment
}

/// The exclusion file's rows by (tool, fixture); none when the variable is unset.
fn exclusions() -> BTreeMap<(String, String), String> {
    let Some(path) = std::env::var_os(EXCLUDE_ENV).filter(|path| !path.is_empty()) else {
        return BTreeMap::new();
    };
    println!("cargo::rerun-if-changed={}", Path::new(&path).display());
    let text: String = std::fs::read_to_string(&path).unwrap_or_default();
    parse_exclusions(&text)
        .unwrap_or_else(|e| panic!("{EXCLUDE_ENV}: {e}"))
        .into_iter()
        .map(|e: Exclusion| ((e.tool, e.fixture), e.reason))
        .collect()
}

/// Our codegen of the input's schema: the source and its root type's name.
fn generate(input: &Input) -> Result<(String, String), String> {
    let schema: Schema = serde_json::from_str(&input.schema_text)
        .map_err(|e: serde_json::Error| format!("schema parse error: {e}"))?;
    let settings: CodeGenSettings = CodeGenSettings::builder().build();
    let mut codegen_input: CodeGenInput<'_> = CodeGenInput::new().schema(&schema);
    if !input.resources.is_empty() {
        let mut documents: BTreeMap<String, Value> = BTreeMap::new();
        for (uri, text) in &input.resources {
            let document: Value = serde_json::from_str(text)
                .map_err(|e: serde_json::Error| format!("resource parse error: {e}"))?;
            documents.insert(uri.clone(), document);
        }
        codegen_input = codegen_input.retriever(documents);
    }
    let output: GenerateRustOutput = generate_rust(codegen_input, &settings)
        .map_err(|e| format!("our codegen failed: {}", one_line(&e.to_string())))?;
    if output.shared.is_some() {
        return Err("our codegen produced a shared buffer for one schema".to_owned());
    }
    let source: Vec<u8> = output.per_schema.into_iter().next().unwrap_or_default();
    let root: String = output.root_types.into_iter().next().unwrap_or_default();
    String::from_utf8(source)
        .map(|source: String| (source, root))
        .map_err(|e| format!("generated source is not UTF-8: {e}"))
}

/// Write one tool's model (unless excluded) and return its table entry.
fn model_entry(
    models_dir: &Path,
    modules: &mut String,
    model: &Planned<'_>,
    generated: &str,
    root: &str,
    excluded: &BTreeMap<(String, String), String>,
) -> String {
    let tool: &str = model.tool;
    if let Some(reason) = excluded.get(&(tool.to_owned(), model.input.id.clone())) {
        return format!("Err({reason:?})");
    }
    let source: String = if tool == "schemars" {
        match schemars_source(generated) {
            Ok(source) => source,
            Err(reason) => return format!("Err({:?})", format!("no model: {reason}")),
        }
    } else {
        generated.to_owned()
    };
    let dir: PathBuf = models_dir.join(tool).join(model_dir_name(&model.input.id));
    std::fs::create_dir_all(&dir).expect("cannot create a model directory");
    let file: PathBuf = dir.join(MODEL_FILE_NAME);
    std::fs::write(&file, source).expect("cannot write a model");
    let module: String = format!("{}_{}", tool.replace('-', "_"), model.index);
    let _ = writeln!(
        modules,
        "#[doc(hidden)]\n#[path = {:?}]\npub mod {module};",
        file.display().to_string()
    );
    if tool == "schemars" {
        format!("Ok(|| schemars::schema_for!({module}::{root}))")
    } else {
        format!("Ok(<{module}::{root} as json_schema_rs::ToJsonSchema>::json_schema)")
    }
}

/// `generated` with schemars' derive and attributes in place of ours.
fn schemars_source(generated: &str) -> Result<String, String> {
    let mut file: syn::File =
        syn::parse_file(generated).map_err(|e| format!("generated source is not Rust: {e}"))?;
    let mut translator: Translator = Translator::default();
    translator.visit_file_mut(&mut file);
    if let Some(error) = translator.error {
        return Err(error);
    }
    file.items.extend(translator.extra_items);
    Ok(prettyplease::unparse(&file))
}

/// Rewrites our derive and attributes as schemars', collecting the `transform` functions
/// it adds and the first problem it meets.
#[derive(Default)]
struct Translator {
    extra_items: Vec<syn::Item>,
    error: Option<String>,
}

impl Translator {
    fn fail(&mut self, error: String) {
        self.error.get_or_insert(error);
    }
}

impl VisitMut for Translator {
    fn visit_item_struct_mut(&mut self, item: &mut syn::ItemStruct) {
        replace_derive(&mut item.attrs);
        let container: Constraints = match take_constraints(&mut item.attrs) {
            Ok(constraints) => constraints,
            Err(e) => return self.fail(e),
        };
        // A newtype's value constraints describe its one field.
        let newtype: bool =
            matches!(&item.fields, syn::Fields::Unnamed(fields) if fields.unnamed.len() == 1);
        for field in &mut item.fields {
            if let Err(e) = translate_field(field) {
                return self.fail(e);
            }
        }
        if newtype {
            let (value, rest): (Constraints, Constraints) = container.split_value();
            if let Some(field) = item.fields.iter_mut().next() {
                push_schemars(&mut field.attrs, &value);
            }
            push_schemars(&mut item.attrs, &rest);
        } else {
            push_schemars(&mut item.attrs, &container.into_extensions());
        }
    }

    fn visit_item_enum_mut(&mut self, item: &mut syn::ItemEnum) {
        replace_derive(&mut item.attrs);
        let container: Constraints = match take_constraints(&mut item.attrs) {
            Ok(constraints) => constraints,
            Err(e) => return self.fail(e),
        };
        let mut values: Vec<String> = Vec::new();
        let mut has_values: bool = false;
        for variant in &mut item.variants {
            let mut constraints: Constraints = match take_constraints(&mut variant.attrs) {
                Ok(constraints) => constraints,
                Err(e) => return self.fail(e),
            };
            match constraints.value.take() {
                Some(value) => {
                    has_values = true;
                    values.push(value);
                }
                None => values.push(format!("{:?}", serde_name(variant))),
            }
            push_schemars(&mut variant.attrs, &constraints);
            for field in &mut variant.fields {
                if let Err(e) = translate_field(field) {
                    return self.fail(e);
                }
            }
        }
        let mut container: Constraints = container.into_extensions();
        if has_values {
            let name: String = format!("schemars_enum_values_{}", self.extra_items.len());
            let function: String = format!(
                "fn {name}(schema: &mut schemars::Schema) {{\n\
                 let object = schema.ensure_object();\n\
                 object.remove(\"type\");\n\
                 object.remove(\"oneOf\");\n\
                 object.insert(\"enum\".to_owned(), serde_json::json!([{}]));\n}}",
                values.join(", ")
            );
            match syn::parse_str::<syn::Item>(&function) {
                Ok(function) => self.extra_items.push(function),
                Err(e) => return self.fail(format!("cannot express enum values: {e}")),
            }
            container.schemars.push(format!("transform = {name}"));
        }
        push_schemars(&mut item.attrs, &container);
    }
}

/// Translate one field's `#[json_schema(...)]` into `#[schemars(...)]`.
fn translate_field(field: &mut syn::Field) -> Result<(), String> {
    let constraints: Constraints = take_constraints(&mut field.attrs)?;
    push_schemars(&mut field.attrs, &constraints);
    Ok(())
}

/// The name serde gives a variant: its `#[serde(rename = "...")]`, else its identifier.
fn serde_name(variant: &syn::Variant) -> String {
    for attr in &variant.attrs {
        if !attr.path().is_ident("serde") {
            continue;
        }
        let mut rename: Option<String> = None;
        let _ = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("rename") {
                let lit: syn::LitStr = meta.value()?.parse()?;
                rename = Some(lit.value());
            } else if meta.input.peek(syn::Token![=]) {
                let _ignored: syn::Expr = meta.value()?.parse()?;
            }
            Ok(())
        });
        if let Some(rename) = rename {
            return rename;
        }
    }
    variant.ident.to_string()
}

/// `#[derive(..., json_schema_rs_macro::ToJsonSchema, ...)]` with `schemars::JsonSchema`.
fn replace_derive(attrs: &mut [syn::Attribute]) {
    for attr in attrs.iter_mut() {
        if !attr.path().is_ident("derive") {
            continue;
        }
        let Ok(paths) =
            attr.parse_args_with(Punctuated::<syn::Path, syn::Token![,]>::parse_terminated)
        else {
            continue;
        };
        let names: Vec<String> = paths
            .iter()
            .map(|path: &syn::Path| {
                let text: String = path_text(path);
                if text == "json_schema_rs_macro::ToJsonSchema" {
                    "schemars::JsonSchema".to_owned()
                } else {
                    text
                }
            })
            .collect();
        if let Ok(replaced) = parse_attribute(&format!("#[derive({})]", names.join(", "))) {
            *attr = replaced;
        }
    }
}

/// A path as written, segments joined by `::`.
fn path_text(path: &syn::Path) -> String {
    path.segments
        .iter()
        .map(|segment: &syn::PathSegment| segment.ident.to_string())
        .collect::<Vec<String>>()
        .join("::")
}

/// One outer attribute from its text.
fn parse_attribute(text: &str) -> Result<syn::Attribute, String> {
    use syn::parse::Parser as _;
    syn::Attribute::parse_outer
        .parse_str(text)
        .map_err(|e| format!("cannot build {text}: {e}"))?
        .into_iter()
        .next()
        .ok_or_else(|| format!("cannot build {text}"))
}

/// The constraints of one item, field or variant, in schemars' terms.
#[derive(Default)]
struct Constraints {
    /// `length(min = .., max = ..)` bounds.
    length: (Option<String>, Option<String>),
    /// `range(min = .., max = ..)` bounds.
    range: (Option<String>, Option<String>),
    /// `regex(pattern = ..)`.
    pattern: Option<String>,
    /// Other `#[schemars(...)]` items as written (`title = ..`, `required`, `transform`).
    schemars: Vec<String>,
    /// `extend("key" = value)` items.
    extend: Vec<(String, String)>,
    /// An enum variant's literal value.
    value: Option<String>,
}

impl Constraints {
    /// (the value constraints a newtype puts on its field, everything else).
    fn split_value(self) -> (Constraints, Constraints) {
        let value: Constraints = Constraints {
            length: self.length,
            range: self.range,
            pattern: self.pattern,
            ..Constraints::default()
        };
        let rest: Constraints = Constraints {
            schemars: self.schemars,
            extend: self.extend,
            value: self.value,
            ..Constraints::default()
        };
        (value, rest)
    }

    /// The constraints with each value bound restated as a raw keyword (`length` and
    /// `range` apply to a value, not to a struct or enum container).
    fn into_extensions(mut self) -> Constraints {
        let pairs: [(&str, Option<String>); 4] = [
            ("minLength", self.length.0.take()),
            ("maxLength", self.length.1.take()),
            ("minimum", self.range.0.take()),
            ("maximum", self.range.1.take()),
        ];
        for (key, value) in pairs {
            if let Some(value) = value {
                self.extend.push((key.to_owned(), value));
            }
        }
        if let Some(pattern) = self.pattern.take() {
            self.extend.push(("pattern".to_owned(), pattern));
        }
        self
    }

    /// The `#[schemars(...)]` items, empty when there are none.
    fn items(&self) -> Vec<String> {
        let mut items: Vec<String> = Vec::new();
        for (name, (min, max)) in [("length", &self.length), ("range", &self.range)] {
            let bounds: Vec<String> = [("min", min), ("max", max)]
                .into_iter()
                .filter_map(|(key, value)| value.as_ref().map(|v| format!("{key} = {v}")))
                .collect();
            if !bounds.is_empty() {
                items.push(format!("{name}({})", bounds.join(", ")));
            }
        }
        if let Some(pattern) = &self.pattern {
            items.push(format!("regex(pattern = {pattern})"));
        }
        items.extend(self.schemars.iter().cloned());
        if !self.extend.is_empty() {
            let pairs: Vec<String> = self
                .extend
                .iter()
                .map(|(key, value)| format!("{key:?} = {value}"))
                .collect();
            items.push(format!("extend({})", pairs.join(", ")));
        }
        items
    }
}

/// Append `#[schemars(...)]` for `constraints`, when it has any.
fn push_schemars(attrs: &mut Vec<syn::Attribute>, constraints: &Constraints) {
    let items: Vec<String> = constraints.items();
    if items.is_empty() {
        return;
    }
    let text: String = format!("#[schemars({})]", items.join(", "));
    attrs.push(parse_attribute(&text).unwrap_or_else(|e| panic!("{e}")));
}

/// Remove every `#[json_schema(...)]` from `attrs`, returning what they say.
fn take_constraints(attrs: &mut Vec<syn::Attribute>) -> Result<Constraints, String> {
    let mut constraints: Constraints = Constraints::default();
    let mut kept: Vec<syn::Attribute> = Vec::with_capacity(attrs.len());
    for attr in attrs.drain(..) {
        if !attr.path().is_ident("json_schema") {
            kept.push(attr);
            continue;
        }
        let metas: Punctuated<syn::Meta, syn::Token![,]> = attr
            .parse_args_with(Punctuated::parse_terminated)
            .map_err(|e| format!("cannot read #[json_schema(...)]: {e}"))?;
        for meta in metas {
            let key: String = path_text(meta.path());
            let value: Option<String> = match &meta {
                syn::Meta::NameValue(pair) => Some(expr_text(&pair.value)),
                syn::Meta::Path(_) => None,
                syn::Meta::List(list) => Some(list.tokens.to_string()),
            };
            constraints.add(&key, value);
        }
    }
    *attrs = kept;
    Ok(constraints)
}

impl Constraints {
    /// Record one `#[json_schema(key = value)]` (or bare `key`).
    fn add(&mut self, key: &str, value: Option<String>) {
        let flag: String = value.clone().unwrap_or_else(|| "true".to_owned());
        match key {
            "min_length" | "min_items" => self.length.0 = value,
            "max_length" | "max_items" => self.length.1 = value,
            "minimum" => self.range.0 = value.as_deref().map(typed_number),
            "maximum" => self.range.1 = value.as_deref().map(typed_number),
            "pattern" => self.pattern = value,
            "title" | "description" => self.schemars.push(format!("{key} = {flag}")),
            "required" => self.schemars.push("required".to_owned()),
            "value" => self.value = value,
            // An untagged enum is schemars' `anyOf`; it has no `oneOf` form.
            "one_of" => {}
            "id" => self.extend.push(("$id".to_owned(), flag)),
            "comment" => self.extend.push(("$comment".to_owned(), flag)),
            other => self.extend.push((camel_case(other), flag)),
        }
    }
}

/// A numeric bound as a typed literal: schemars infers an unsuffixed integer as `i32`, so a
/// bound beyond it needs the suffix a schemars user would write (`i64`, or `u64` above
/// `i64::MAX`). A float stays as written.
fn typed_number(text: &str) -> String {
    let compact: String = text.split_whitespace().collect();
    if let Ok(n) = compact.parse::<i64>() {
        format!("{n}_i64")
    } else if let Ok(n) = compact.parse::<u64>() {
        format!("{n}_u64")
    } else {
        compact
    }
}

/// A Rust expression as source text (a literal keeps its spelling).
fn expr_text(expr: &syn::Expr) -> String {
    use quote::ToTokens as _;
    expr.to_token_stream().to_string()
}

/// `snake_case` as `camelCase` (`unique_items` -> `uniqueItems`).
fn camel_case(snake: &str) -> String {
    let mut out: String = String::with_capacity(snake.len());
    let mut upper: bool = false;
    for c in snake.chars() {
        if c == '_' {
            upper = true;
        } else if upper {
            out.extend(c.to_uppercase());
            upper = false;
        } else {
            out.push(c);
        }
    }
    out
}

/// Collapse whitespace runs to single spaces.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<&str>>().join(" ")
}
