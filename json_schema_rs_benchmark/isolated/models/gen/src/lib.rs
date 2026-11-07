//! Generator of the benchmark's models, the build-dependency of every model shard crate
//! (`models/shard_<n>`; see `isolated/Cargo.toml`).
//!
//! The models: for every measurable fixture (and the CLI tier's startup baseline), the Rust types
//! our codegen generates for its schema (its resources reachable through an in-memory retriever,
//! as in the codegen benchmark), in the copies the shard's features ask for ([`ModelCopy`]):
//!
//! * `jsonschemars` (both features): the generated source as is, default settings. It derives our
//!   `ToJsonSchema` (`reverse-codegen`) and is the type every generated-types operation of the
//!   default settings deserializes into (`generated-types`).
//! * `schemars` (`reverse-codegen`): the same source with `schemars::JsonSchema` in place of our
//!   derive, and each `#[json_schema(...)]` constraint restated as the `#[schemars(...)]`
//!   attribute a schemars user would write (`length`, `range`, `regex`, `title`, `description`,
//!   `required`, an enum's literal values through a `transform`, and every other keyword through
//!   `extend`), so both derives describe the same schema. A one-of union keeps schemars' own
//!   `anyOf` for an untagged enum.
//! * `jsonschemars-no-newtypes` (`generated-types`): our codegen with
//!   `constrained_newtypes(false)`, for the `*_no_newtypes` operations.
//!
//! A fixture in `MODELS_NOT_BUILT` (`json_schema_rs_benchmark::models`) has no model in any copy:
//! its types take too long to build (#102, #111). Its tables' rows hold the reason.
//!
//! **Shards.** The models are about 400K lines of generated Rust. In one crate they compile one
//! after another (rustc's front end is serial within a crate, and the benchmark profile has one
//! codegen unit), so they are spread over [`SHARDS`] crates that cargo compiles in parallel, each
//! with the same benchmark profile and features. Every shard's build script generates every
//! fixture's models (our codegen is fast) and assigns the (copy, fixture) models to shards the
//! same way: largest first, each to the shard with the fewest lines so far. A shard then writes
//! only its own models. The build then takes about as long as its slowest model (the reverse
//! codegen models took 43 minutes in one crate); `MODELS_NOT_BUILT` leaves out the models that
//! take minutes or more on their own (#102, #111).
//!
//! Each copy is a module of its own (`$OUT_DIR/models/<copy>/<fixture>/model.rs`), so a
//! model that does not compile fails only that model. The driver turns such a build failure
//! into rows of the exclusion file (`BENCH_MODEL_EXCLUDE`, see
//! `json_schema_rs_benchmark::models`), and an excluded model is left out, its table
//! entries holding the reason. A fixture whose codegen fails has no model for any copy.

use json_schema_rs::{CodeGenInput, CodeGenSettings, GenerateRustOutput, Schema, generate_rust};
use json_schema_rs_benchmark::fixtures::{self, Fixture};
use json_schema_rs_benchmark::models::{
    EXCLUDE_ENV, Exclusion, MODEL_FILE_NAME, MODELS_DIR_NAME, STARTUP_FIXTURE_ID, STARTUP_SCHEMA,
    model_dir_name, model_not_built, parse_exclusions,
};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use syn::punctuated::Punctuated;
use syn::visit_mut::VisitMut;

/// Number of model shard crates (`models/shard_0` .. `shard_7`). The largest model is
/// about a sixth of all model lines and the slowest one most of the build's time, so more shards
/// would not shorten it.
pub const SHARDS: usize = 8;

/// One copy of a fixture's model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum ModelCopy {
    /// Our codegen, default settings.
    Ours,
    /// Our codegen's default output, with schemars' derive in place of ours.
    Schemars,
    /// Our codegen with `constrained_newtypes(false)`.
    OursNoNewtypes,
}

impl ModelCopy {
    /// The copy's id: its directory name, and the tool id of its exclusion rows.
    const fn id(self) -> &'static str {
        match self {
            ModelCopy::Ours => "jsonschemars",
            ModelCopy::Schemars => "schemars",
            ModelCopy::OursNoNewtypes => "jsonschemars-no-newtypes",
        }
    }

    /// Whether the copy is our codegen's output with constrained newtypes on.
    const fn newtypes(self) -> bool {
        !matches!(self, ModelCopy::OursNoNewtypes)
    }
}

/// The features of the shard being built (cargo sets `CARGO_FEATURE_<NAME>` for each).
#[derive(Debug, Clone, Copy)]
struct Features {
    /// `reverse-codegen`: our derive's and schemars' models.
    reverse_codegen: bool,
    /// `generated-types`: the generated-types operations' entry points, both settings.
    generated_types: bool,
}

impl Features {
    fn from_env() -> Features {
        Features {
            reverse_codegen: std::env::var_os("CARGO_FEATURE_REVERSE_CODEGEN").is_some(),
            generated_types: std::env::var_os("CARGO_FEATURE_GENERATED_TYPES").is_some(),
        }
    }

    /// The copies these features need, in a fixed order.
    fn copies(self) -> Vec<ModelCopy> {
        let mut copies: Vec<ModelCopy> = Vec::new();
        if self.reverse_codegen || self.generated_types {
            copies.push(ModelCopy::Ours);
        }
        if self.reverse_codegen {
            copies.push(ModelCopy::Schemars);
        }
        if self.generated_types {
            copies.push(ModelCopy::OursNoNewtypes);
        }
        copies
    }
}

/// A model to build: the fixture id, and its schema text and resources.
struct Input {
    id: String,
    schema_text: String,
    resources: BTreeMap<String, String>,
}

/// Our codegen's output for one input: the source and its root type's name, or why there is
/// none.
type Generated = Result<(String, String), String>;

/// One (copy, fixture) model before it is written: its source, or why there is none.
struct Planned<'a> {
    copy: ModelCopy,
    index: usize,
    input: &'a Input,
    generated: Result<&'a (String, String), &'a str>,
}

/// Build script of shard crate `CARGO_PKG_NAME` (`models-shard-<n>`): writes the shard's
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
        .unwrap_or_else(|| panic!("{package} is not named models-shard-<0..{SHARDS}>"));
    let out_dir: PathBuf = PathBuf::from(std::env::var("OUT_DIR").expect("cargo sets OUT_DIR"));
    std::fs::write(
        out_dir.join("models.rs"),
        source(shard, &out_dir, Features::from_env()),
    )
    .expect("cannot write models.rs");
}

/// The tables of one shard's generated module, as Rust source lines.
#[derive(Default)]
struct Tables {
    /// `JSONSCHEMARS`: our derive's root-schema function per fixture.
    reverse_ours: String,
    /// `SCHEMARS`: schemars' root-schema function per fixture.
    reverse_schemars: String,
    /// `TYPES`: the default settings' generated types per fixture.
    types: String,
    /// `TYPES_NO_NEWTYPES`: the no-newtypes generated types per fixture.
    types_no_newtypes: String,
}

/// The shard's generated module: its models' `mod` declarations and the tables its features
/// ask for (fixture id to the model's entry point, or why it has none).
fn source(shard: usize, out_dir: &Path, features: Features) -> String {
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
    let copies: Vec<ModelCopy> = features.copies();
    let with_newtypes: Vec<Generated> = inputs.iter().map(|i| generate(i, true)).collect();
    let without_newtypes: Vec<Generated> = if copies.contains(&ModelCopy::OursNoNewtypes) {
        inputs.iter().map(|i| generate(i, false)).collect()
    } else {
        Vec::new()
    };
    let mut planned: Vec<Planned<'_>> = Vec::new();
    for (index, input) in inputs.iter().enumerate() {
        for copy in copies.iter().copied() {
            let generated: &Generated = if copy.newtypes() {
                &with_newtypes[index]
            } else {
                &without_newtypes[index]
            };
            planned.push(Planned {
                copy,
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
    let mut tables: Tables = Tables::default();
    for (model, _shard) in planned
        .iter()
        .zip(&assignment)
        .filter(|(_model, assigned): &(&Planned<'_>, &usize)| **assigned == shard)
    {
        let written: Result<(String, String), String> = match model.generated {
            Ok((source, root)) => write_model(&models_dir, &mut modules, model, source, &excluded)
                .map(|(module, try_from)| (module, entry_types(root, try_from))),
            Err(reason) => Err(format!("no model: {reason}")),
        };
        add_entries(&mut tables, model, &written, features);
    }
    modules + &render_tables(&tables, features)
}

/// The `pub static` tables `features` ask for.
fn render_tables(tables: &Tables, features: Features) -> String {
    let mut out: String = String::new();
    let mut table = |doc: &str, name: &str, ty: &str, rows: &str| {
        let _ = write!(
            out,
            "\n/// {doc}, by fixture id.\npub static {name}: &[(&str, {ty})] = &[\n{rows}];\n"
        );
    };
    if features.reverse_codegen {
        table(
            "Our derive's models in this shard",
            "JSONSCHEMARS",
            "OurModel",
            &tables.reverse_ours,
        );
        table(
            "schemars' models in this shard",
            "SCHEMARS",
            "SchemarsModel",
            &tables.reverse_schemars,
        );
    }
    if features.generated_types {
        table(
            "The default settings' generated types in this shard",
            "TYPES",
            "Types",
            &tables.types,
        );
        table(
            "The no-newtypes generated types in this shard",
            "TYPES_NO_NEWTYPES",
            "Types",
            &tables.types_no_newtypes,
        );
    }
    out
}

/// Add `model`'s rows to the tables its copy fills: `written` is the model's module and the
/// generated-types expression of its root type, or why it has none.
fn add_entries(
    tables: &mut Tables,
    model: &Planned<'_>,
    written: &Result<(String, String), String>,
    features: Features,
) {
    let id: &str = &model.input.id;
    let row = |table: &mut String, entry: String| {
        let _ = writeln!(table, "    ({id:?}, {entry}),");
    };
    let root: &str = model.generated.map_or("", |(_source, root)| root.as_str());
    match model.copy {
        ModelCopy::Ours => {
            if features.reverse_codegen {
                row(
                    &mut tables.reverse_ours,
                    match written {
                        Ok((module, _types)) => format!(
                            "Ok(<{module}::{root} as json_schema_rs::ToJsonSchema>::json_schema)"
                        ),
                        Err(reason) => format!("Err({reason:?})"),
                    },
                );
            }
            if features.generated_types {
                row(&mut tables.types, types_entry(written));
            }
        }
        ModelCopy::Schemars => row(
            &mut tables.reverse_schemars,
            match written {
                Ok((module, _types)) => format!("Ok(|| schemars::schema_for!({module}::{root}))"),
                Err(reason) => format!("Err({reason:?})"),
            },
        ),
        ModelCopy::OursNoNewtypes => {
            row(&mut tables.types_no_newtypes, types_entry(written));
        }
    }
}

/// A `TYPES` row's entry: the generated-types expression with the model's module filled in.
fn types_entry(written: &Result<(String, String), String>) -> String {
    match written {
        Ok((module, types)) => format!("Ok({})", types.replace("{module}", module)),
        Err(reason) => format!("Err({reason:?})"),
    }
}

/// The `GeneratedTypes` expression of root type `root` (in module `{module}`, filled in later).
fn entry_types(root: &str, try_from: Option<&'static str>) -> String {
    match try_from {
        None => format!(
            "json_schema_rs_benchmark::harness::GeneratedTypes::with_try_from::<{{module}}::{root}>()"
        ),
        Some(reason) => format!(
            "json_schema_rs_benchmark::harness::GeneratedTypes::without_try_from::<{{module}}::{root}>({reason:?})"
        ),
    }
}

/// The shard of each planned model: largest first (by lines of its generated source; a model
/// without source weighs nothing), each to the least-loaded shard, ties to the lowest. The
/// order depends only on the fixtures and the features, so every shard's build script computes
/// the same assignment.
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
            .then_with(|| planned[*a].copy.cmp(&planned[*b].copy))
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

/// The exclusion file's rows by (copy id, fixture); none when the variable is unset.
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

/// Our codegen of the input's schema (default settings, with `constrained_newtypes(newtypes)`):
/// the source and its root type's name; none for a fixture in `MODELS_NOT_BUILT`.
fn generate(input: &Input, newtypes: bool) -> Generated {
    if let Some(reason) = model_not_built(&input.id) {
        return Err(reason.to_owned());
    }
    let schema: Schema = serde_json::from_str(&input.schema_text)
        .map_err(|e: serde_json::Error| format!("schema parse error: {e}"))?;
    let settings: CodeGenSettings = CodeGenSettings::builder()
        .constrained_newtypes(newtypes)
        .build();
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

/// Write one model (unless excluded): returns its module name and, for our copies, why the
/// root type has no `TryFrom<serde_json::Value>` (`None` when it has one), or why the model was
/// left out.
fn write_model(
    models_dir: &Path,
    modules: &mut String,
    model: &Planned<'_>,
    generated: &str,
    excluded: &BTreeMap<(String, String), String>,
) -> Result<(String, Option<&'static str>), String> {
    let copy: ModelCopy = model.copy;
    if let Some(reason) = excluded.get(&(copy.id().to_owned(), model.input.id.clone())) {
        return Err(reason.clone());
    }
    let root: &str = model.generated.map_or("", |(_source, root)| root.as_str());
    let (source, try_from): (String, Option<&'static str>) = if copy == ModelCopy::Schemars {
        (
            schemars_source(generated).map_err(|reason| format!("no model: {reason}"))?,
            None,
        )
    } else {
        (generated.to_owned(), root_try_from(generated, root)?)
    };
    let dir: PathBuf = models_dir
        .join(copy.id())
        .join(model_dir_name(&model.input.id));
    std::fs::create_dir_all(&dir).expect("cannot create a model directory");
    let file: PathBuf = dir.join(MODEL_FILE_NAME);
    std::fs::write(&file, source).expect("cannot write a model");
    let module: String = format!("{}_{}", copy.id().replace('-', "_"), model.index);
    let _ = writeln!(
        modules,
        "#[doc(hidden)]\n#[path = {:?}]\npub mod {module};",
        file.display().to_string()
    );
    Ok((module, try_from))
}

/// Why root type `root` of `generated` has no `TryFrom<serde_json::Value>` (self-validating types
/// give one to every struct, enum and union with a validation target; a root type alias or a
/// constrained newtype has none), or `None` when it has one.
fn root_try_from(generated: &str, root: &str) -> Result<Option<&'static str>, String> {
    let file: syn::File = syn::parse_file(generated)
        .map_err(|e| format!("no model: generated source is not Rust: {e}"))?;
    let has_try_from: bool = file.items.iter().any(|item: &syn::Item| {
        let syn::Item::Impl(item) = item else {
            return false;
        };
        let Some((trait_path, _for)) = &item.trait_ else {
            return false;
        };
        let self_is_root: bool = matches!(
            item.self_ty.as_ref(),
            syn::Type::Path(ty) if ty.qself.is_none() && ty.path.is_ident(root)
        );
        let names_try_from_value: bool = trait_path.segments.last().is_some_and(|segment| {
            segment.ident == "TryFrom" && {
                use quote::ToTokens as _;
                segment
                    .arguments
                    .to_token_stream()
                    .to_string()
                    .replace(' ', "")
                    == "<serde_json::Value>"
            }
        });
        self_is_root && names_try_from_value
    });
    Ok(if has_try_from {
        None
    } else {
        Some("the root type has no TryFrom<serde_json::Value>")
    })
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
