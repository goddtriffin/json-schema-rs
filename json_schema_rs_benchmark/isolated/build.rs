//! Generates the `$OUT_DIR` modules of this crate's feature builds from the fixture set:
//!
//! * `macro_validators.rs` (`jsonschema-macros`): one `#[jsonschema::validator]` struct per
//!   fixture, plus the `MACRO_VALIDATORS` table that maps each fixture id to its validator (or to
//!   why it has none). The macro reports a schema it cannot compile as a compile error, which
//!   would fail the whole build, so each schema is first compiled with the same release's runtime
//!   validator, through a registry holding the fixture's resources: a schema it rejects gets a
//!   table entry with the rejection instead of a validator. Without the feature the table is
//!   empty.
//! * `reverse_models.rs` (`reverse-codegen`): the reverse-codegen models (see the `reverse`
//!   module below).
//!
//! Fixtures come from the same discovery as every other tool (`BENCH_FIXTURE_ROOTS`, or the
//! committed and generated fixture directories). A fixture with a `$ref` to a document it does
//! not vendor gets nothing: the harness never measures it.

use std::path::PathBuf;

fn main() {
    let out_dir: PathBuf = PathBuf::from(std::env::var("OUT_DIR").expect("cargo sets OUT_DIR"));
    std::fs::write(out_dir.join("macro_validators.rs"), macros::source())
        .expect("cannot write macro_validators.rs");
    #[cfg(feature = "reverse-codegen")]
    std::fs::write(out_dir.join("reverse_models.rs"), reverse::source(&out_dir))
        .expect("cannot write reverse_models.rs");
}

#[cfg(not(feature = "jsonschema-macros"))]
mod macros {
    /// The generated module of a build without compile-time validators.
    pub(crate) fn source() -> String {
        "/// Compile-time validators by fixture id (none in this build).\n\
         pub static MACRO_VALIDATORS: &[(&str, MacroValidator)] = &[];\n"
            .to_owned()
    }
}

#[cfg(feature = "jsonschema-macros")]
mod macros {
    use json_schema_rs_benchmark::fixtures::{self, FIXTURE_ROOTS_ENV, Fixture, SCHEMA_FILE_NAME};
    use std::fmt::Write as _;
    use std::path::PathBuf;

    /// The generated module of the `jsonschema-macros` build, from every discovered fixture.
    pub(crate) fn source() -> String {
        println!("cargo::rerun-if-env-changed={FIXTURE_ROOTS_ENV}");
        // Cargo scans a watched directory recursively, so adding, removing or editing any fixture
        // reruns this script. A root that does not exist is not watched (Cargo would rerun on
        // every build).
        for root in fixtures::configured_roots() {
            if root.is_dir() {
                println!("cargo::rerun-if-changed={}", root.display());
            }
        }
        let found: Vec<Fixture> = fixtures::discover_configured()
            .unwrap_or_else(|e| panic!("fixture discovery failed: {e}"));
        render(&found)
    }

    /// The generated module: one validator struct per measurable fixture and the lookup table.
    fn render(found: &[Fixture]) -> String {
        let mut structs: String = String::new();
        let mut table: String = String::new();
        for (index, fixture) in found.iter().enumerate() {
            if !fixture.unresolved_refs().is_empty() {
                continue;
            }
            let schema_path: PathBuf = fixture.dir.join(SCHEMA_FILE_NAME);
            if let Err(reason) = runtime_compile(fixture) {
                let _ = writeln!(
                    table,
                    "    ({:?}, MacroValidator::Rejected({:?})),",
                    fixture.id, reason
                );
                continue;
            }
            let mut resources: String = String::new();
            for resource in &fixture.resources {
                let _ = write!(
                    resources,
                    "{:?} => {{ path = {:?} }}, ",
                    resource.uri,
                    resource.path.display().to_string()
                );
            }
            let _ = writeln!(
                structs,
                "#[jsonschema::validator(path = {:?}, resources = {{ {resources}}})]\n\
             struct Validator{index};\n",
                schema_path.display().to_string()
            );
            let _ = writeln!(
                table,
                "    ({:?}, MacroValidator::Built(|instance: &serde_json::Value| {{\n\
             \x20       let errors: Vec<jsonschema::ValidationError<'_>> =\n\
             \x20           Validator{index}::iter_errors(instance).collect();\n\
             \x20       errors.is_empty()\n\
             \x20   }})),",
                fixture.id
            );
        }
        format!(
            "{structs}/// Compile-time validators by fixture id.\n\
         pub static MACRO_VALIDATORS: &[(&str, MacroValidator)] = &[\n{table}];\n"
        )
    }

    /// Compile the fixture's schema with jsonschema's runtime validator, its resources registered
    /// under their URIs, as the main crate's `jsonschema` adapter does.
    fn runtime_compile(fixture: &Fixture) -> Result<(), String> {
        let parse = |text: &str| -> Result<serde_json::Value, String> {
            serde_json::from_str(text).map_err(|e: serde_json::Error| e.to_string())
        };
        let schema: serde_json::Value = parse(&fixture.schema_text)?;
        let mut builder: jsonschema::RegistryBuilder<'_> = jsonschema::Registry::new();
        for resource in &fixture.resources {
            builder = builder
                .add(resource.uri.as_str(), parse(&resource.text)?)
                .map_err(|e: jsonschema::ReferencingError| e.to_string())?;
        }
        let registry: jsonschema::Registry<'_> = builder
            .prepare()
            .map_err(|e: jsonschema::ReferencingError| e.to_string())?;
        jsonschema::options()
            .with_registry(&registry)
            .build(&schema)
            .map(|_validator: jsonschema::Validator| ())
            .map_err(|e: jsonschema::ValidationError<'_>| e.to_string())
    }
}

/// The reverse-codegen models: for every measurable fixture (and the CLI tier's startup baseline),
/// the Rust types our codegen generates for its schema (default settings, its resources reachable
/// through an in-memory retriever, as in the codegen benchmark), in one copy per tool:
///
/// * `jsonschemars`: the generated source as is (it derives our `ToJsonSchema`).
/// * `schemars`: the same source with `schemars::JsonSchema` in place of our derive, and each
///   `#[json_schema(...)]` constraint restated as the `#[schemars(...)]` attribute a schemars
///   user would write (`length`, `range`, `regex`, `title`, `description`, `required`, an
///   enum's literal values through a `transform`, and every other keyword through `extend`), so
///   both derives describe the same schema. A one-of union keeps schemars' own `anyOf` for an
///   untagged enum.
///
/// Each copy is a module of its own (`$OUT_DIR/reverse_models/<tool>/<fixture>/model.rs`), so a
/// derive that cannot handle a model fails only that model. The driver turns such a build failure
/// into rows of the exclusion file (`BENCH_REVERSE_EXCLUDE`, see
/// `json_schema_rs_benchmark::reverse_models`), and an excluded model is left out, its table
/// entry holding the reason. A fixture whose codegen fails has no model for any tool.
#[cfg(feature = "reverse-codegen")]
mod reverse {
    use json_schema_rs::{
        CodeGenInput, CodeGenSettings, GenerateRustOutput, Schema, generate_rust,
    };
    use json_schema_rs_benchmark::fixtures::{self, Fixture};
    use json_schema_rs_benchmark::reverse_models::{
        EXCLUDE_ENV, Exclusion, MODEL_FILE_NAME, MODELS_DIR_NAME, STARTUP_FIXTURE_ID,
        STARTUP_SCHEMA, model_dir_name, parse_exclusions,
    };
    use serde_json::Value;
    use std::collections::BTreeMap;
    use std::fmt::Write as _;
    use std::path::{Path, PathBuf};
    use syn::punctuated::Punctuated;
    use syn::visit_mut::VisitMut;

    /// Tool ids, in table order; the first is ours.
    const TOOLS: [&str; 2] = ["jsonschemars", "schemars"];

    /// A model to build: the fixture id, and its schema text and resources.
    struct Input {
        id: String,
        schema_text: String,
        resources: BTreeMap<String, String>,
    }

    /// The generated module: every model's `mod` declaration and the `REVERSE_MODELS` table.
    pub(crate) fn source(out_dir: &Path) -> String {
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
        let found: Vec<Fixture> = fixtures::discover_configured()
            .unwrap_or_else(|e| panic!("fixture discovery failed: {e}"));
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
        let models_dir: PathBuf = out_dir.join(MODELS_DIR_NAME);
        if models_dir.exists() {
            std::fs::remove_dir_all(&models_dir).expect("cannot clear the models directory");
        }
        let mut modules: String = String::new();
        let mut table: String = String::new();
        for (index, input) in inputs.iter().enumerate() {
            let entries: Vec<String> = match generate(input) {
                Ok((generated, root)) => TOOLS
                    .iter()
                    .map(|tool: &&str| {
                        model_entry(
                            &models_dir,
                            &mut modules,
                            tool,
                            index,
                            input,
                            &generated,
                            &root,
                            &excluded,
                        )
                    })
                    .collect(),
                Err(reason) => TOOLS
                    .iter()
                    .map(|_tool: &&str| format!("Err({:?})", format!("no model: {reason}")))
                    .collect(),
            };
            let _ = writeln!(
                table,
                "    ({:?}, ReverseModels {{\n        jsonschemars: {},\n        schemars: {},\n    }}),",
                input.id, entries[0], entries[1]
            );
        }
        format!(
            "{modules}\n/// The reverse-codegen models by fixture id: each tool's root-schema \
             function, or why it has none.\npub static REVERSE_MODELS: &[(&str, ReverseModels)] = \
             &[\n{table}];\n"
        )
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

    /// Write one tool's model of `input` (unless excluded) and return its table entry.
    #[expect(
        clippy::too_many_arguments,
        reason = "one call site, every argument distinct"
    )]
    fn model_entry(
        models_dir: &Path,
        modules: &mut String,
        tool: &str,
        index: usize,
        input: &Input,
        generated: &str,
        root: &str,
        excluded: &BTreeMap<(String, String), String>,
    ) -> String {
        if let Some(reason) = excluded.get(&(tool.to_owned(), input.id.clone())) {
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
        let dir: PathBuf = models_dir.join(tool).join(model_dir_name(&input.id));
        std::fs::create_dir_all(&dir).expect("cannot create a model directory");
        let file: PathBuf = dir.join(MODEL_FILE_NAME);
        std::fs::write(&file, source).expect("cannot write a model");
        let module: String = format!("{}_{index}", tool.replace('-', "_"));
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
}
