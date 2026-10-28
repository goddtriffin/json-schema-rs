//! Deterministic generator for the large synthetic benchmark fixtures.
//!
//! The committed fixtures are small enough that most operations finish in well under a
//! millisecond, which is below the noise floor of end-to-end CLI timing. This binary writes
//! large synthetic fixtures (wide objects, large arrays, deep nesting, many definitions) so
//! every tool's operations clear that noise floor.
//!
//! Usage: `gen_fixtures --output <DIR>` writes
//! `<DIR>/generated/<name>/schema.json`,
//! `<DIR>/generated/<name>/instances/valid/<file>.json`, and
//! `<DIR>/generated/<name>/instances/invalid/<file>.json`.
//!
//! Output is byte-identical on every run: all randomness comes from a fixed-seed `SplitMix64`
//! generator and every JSON object is built in a fixed insertion order. The generator is
//! idempotent: a file is rewritten only when its content differs, and files under
//! `<DIR>/generated/` that the generator does not produce are removed (along with directories
//! left empty).
//!
//! Every schema targets draft 2020-12 and uses only keywords that all benchmarked tools
//! handle: `type`, `properties`, `required`, `items`, `enum`, `minLength`, `maxLength`,
//! `minimum`, `maximum`, `maxItems`, `$defs`, and local `$ref`s into `#/$defs/...`. No schema
//! is recursive. Each invalid instance violates exactly one constraint, placed at the end (or
//! the deepest point) of the document so a validator that stops at the first error still
//! traverses the whole instance.
//!
//! Uses clap's builder API (not derive) because the workspace forbids `allow_attributes`
//! and clap's derive macro emits internal `#[allow(...)]` attributes.

use clap::{Arg, Command};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// The draft every generated schema declares.
const DRAFT_2020_12: &str = "https://json-schema.org/draft/2020-12/schema";

/// Seed for the fixture PRNG. Changing it changes every generated fixture.
const SEED: u64 = 0x6A73_6F6E_5F72_7321;

/// Number of properties in the `wide-object` schema.
const WIDE_OBJECT_PROPERTIES: usize = 2000;

/// Number of records in the `large-array` valid and invalid instances.
const LARGE_ARRAY_RECORDS: usize = 30_000;

/// Nesting depth (number of nested object levels) of the `deep-nesting` schema.
const DEEP_NESTING_LEVELS: usize = 40;

/// Number of layers in the `many-defs` DAG. Each def references defs of the next layer only,
/// so instance nesting depth stays bounded by the layer count.
const MANY_DEFS_LAYERS: usize = 10;

/// Number of defs per layer in the `many-defs` DAG.
const MANY_DEFS_PER_LAYER: usize = 50;

/// Number of root properties in `many-defs`, each referencing one first-layer def.
const MANY_DEFS_ROOT_ENTRIES: usize = 8;

/// Lowercase letters used for random strings.
const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz";

/// `SplitMix64` pseudo-random generator: tiny, fast, and fully deterministic for a seed.
struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z: u64 = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A value in `0..bound` (`bound` must be non-zero). Modulo bias is irrelevant here.
    fn below(&mut self, bound: u64) -> u64 {
        self.next_u64() % bound
    }

    /// A value in `0..bound` as a `usize` index.
    fn index(&mut self, bound: usize) -> usize {
        let bound_u64: u64 = u64::try_from(bound).expect("usize fits in u64");
        usize::try_from(self.below(bound_u64)).expect("value below a usize bound fits in usize")
    }

    /// A value in `low..=high`.
    fn range_u32(&mut self, low: u32, high: u32) -> u32 {
        let span: u64 = u64::from(high - low) + 1;
        low + u32::try_from(self.below(span)).expect("value below a u32 span fits in u32")
    }

    /// A random lowercase ASCII string with a length in `min_len..=max_len`.
    fn lowercase(&mut self, min_len: u32, max_len: u32) -> String {
        let len: u32 = self.range_u32(min_len, max_len);
        (0..len)
            .map(|_| char::from(ALPHABET[self.index(ALPHABET.len())]))
            .collect()
    }
}

/// One generated fixture: a schema plus one valid and one invalid instance.
struct Fixture {
    name: &'static str,
    schema: Value,
    valid_file: &'static str,
    valid: Value,
    invalid_file: &'static str,
    invalid: Value,
}

/// Rendered files of one fixture, keyed by path relative to the `generated/` directory.
struct RenderedFixture {
    name: &'static str,
    schema_bytes: usize,
    instance_bytes: usize,
    files: BTreeMap<PathBuf, String>,
}

/// Serializes `value` as pretty-printed JSON with a trailing newline.
fn to_pretty(value: &Value) -> String {
    let mut text: String = serde_json::to_string_pretty(value).expect("JSON value serializes");
    text.push('\n');
    text
}

/// Serializes `value` as compact JSON with a trailing newline.
fn to_compact(value: &Value) -> String {
    let mut text: String = serde_json::to_string(value).expect("JSON value serializes");
    text.push('\n');
    text
}

impl Fixture {
    /// Renders the schema (pretty) and instances (compact, since they can be several MB).
    fn render(&self) -> RenderedFixture {
        let base: PathBuf = PathBuf::from(self.name);
        let schema_text: String = to_pretty(&self.schema);
        let valid_text: String = to_compact(&self.valid);
        let invalid_text: String = to_compact(&self.invalid);
        let schema_bytes: usize = schema_text.len();
        let instance_bytes: usize = valid_text.len() + invalid_text.len();
        let mut files: BTreeMap<PathBuf, String> = BTreeMap::new();
        files.insert(base.join("schema.json"), schema_text);
        files.insert(
            base.join("instances").join("valid").join(self.valid_file),
            valid_text,
        );
        files.insert(
            base.join("instances")
                .join("invalid")
                .join(self.invalid_file),
            invalid_text,
        );
        RenderedFixture {
            name: self.name,
            schema_bytes,
            instance_bytes,
            files,
        }
    }
}

/// Builds an object schema from ordered `(name, schema)` properties; every property is required.
fn object_schema_all_required(properties: Vec<(String, Value)>) -> Value {
    let required: Vec<Value> = properties
        .iter()
        .map(|(name, _)| Value::String(name.clone()))
        .collect();
    let properties_map: Map<String, Value> = properties.into_iter().collect();
    json!({
        "type": "object",
        "properties": properties_map,
        "required": required,
    })
}

/// A random simple property (schema plus a value satisfying it) of one of six shapes.
fn random_leaf(rng: &mut SplitMix64) -> (Value, Value) {
    match rng.below(6) {
        0 => {
            let min_len: u32 = rng.range_u32(0, 8);
            let max_len: u32 = min_len + rng.range_u32(0, 24);
            let value: String = rng.lowercase(min_len, max_len);
            (
                json!({"type": "string", "minLength": min_len, "maxLength": max_len}),
                Value::String(value),
            )
        }
        1 => {
            let minimum: u32 = rng.range_u32(0, 1000);
            let maximum: u32 = minimum + rng.range_u32(0, 100_000);
            let value: u32 = rng.range_u32(minimum, maximum);
            (
                json!({"type": "integer", "minimum": minimum, "maximum": maximum}),
                json!(value),
            )
        }
        2 => {
            let value: f64 = f64::from(rng.range_u32(0, 1_000_000)) + 0.25;
            (json!({"type": "number"}), json!(value))
        }
        3 => {
            let value: bool = rng.below(2) == 0;
            (json!({"type": "boolean"}), Value::Bool(value))
        }
        4 => {
            let options: Vec<String> = (0..rng.range_u32(2, 6))
                .map(|index| format!("option_{index}"))
                .collect();
            let value: String = options[rng.index(options.len())].clone();
            (
                json!({"type": "string", "enum": options}),
                Value::String(value),
            )
        }
        _ => {
            let max_items: u32 = rng.range_u32(1, 8);
            let items: Vec<Value> = (0..rng.range_u32(0, max_items))
                .map(|_| Value::String(rng.lowercase(1, 12)))
                .collect();
            (
                json!({
                    "type": "array",
                    "items": {"type": "string", "maxLength": 12},
                    "maxItems": max_items,
                }),
                Value::Array(items),
            )
        }
    }
}

/// One object schema with `property_count` properties of mixed simple types; about half are
/// required. The last property is an integer in `0..=100`; the invalid instance sets it to 101.
fn wide_object(rng: &mut SplitMix64, property_count: usize) -> Fixture {
    let mut properties: Map<String, Value> = Map::new();
    let mut required: Vec<Value> = Vec::new();
    let mut valid: Map<String, Value> = Map::new();
    for index in 0..property_count {
        let name: String = format!("field_{index:04}");
        let is_last: bool = index + 1 == property_count;
        let (schema, value): (Value, Value) = if is_last {
            (
                json!({"type": "integer", "minimum": 0, "maximum": 100}),
                json!(50),
            )
        } else {
            random_leaf(rng)
        };
        if is_last || rng.below(2) == 0 {
            required.push(Value::String(name.clone()));
        }
        properties.insert(name.clone(), schema);
        valid.insert(name, value);
    }
    let mut invalid: Map<String, Value> = valid.clone();
    if property_count > 0 {
        invalid.insert(format!("field_{:04}", property_count - 1), json!(101));
    }
    Fixture {
        name: "wide-object",
        schema: json!({
            "$schema": DRAFT_2020_12,
            "title": "WideObject",
            "type": "object",
            "properties": properties,
            "required": required,
        }),
        valid_file: "all-fields.json",
        valid: Value::Object(valid),
        invalid_file: "last-field-above-maximum.json",
        invalid: Value::Object(invalid),
    }
}

/// Record statuses for the `large-array` fixture.
const RECORD_STATUSES: [&str; 4] = ["active", "inactive", "pending", "archived"];

/// One `large-array` record value with id `id`.
fn large_array_record(rng: &mut SplitMix64, id: usize) -> Value {
    let tags: Vec<Value> = (0..rng.range_u32(0, 4))
        .map(|_| Value::String(rng.lowercase(3, 10)))
        .collect();
    json!({
        "id": id,
        "name": rng.lowercase(4, 20),
        "status": RECORD_STATUSES[rng.index(RECORD_STATUSES.len())],
        "priority": rng.range_u32(1, 5),
        "score": f64::from(rng.range_u32(0, 9999)) / 100.0,
        "verified": rng.below(2) == 0,
        "tags": tags,
        "address": {
            "street": format!("{} {}", rng.range_u32(1, 9999), rng.lowercase(4, 14)),
            "city": rng.lowercase(4, 12),
            "postal_code": format!("{:05}", rng.range_u32(0, 99_999)),
        },
    })
}

/// An object whose single required `records` property is an array whose `items` is a `$ref`
/// to a record def; the invalid instance gives the last record an out-of-range `priority`.
///
/// The array sits under an object root rather than at the root because `jsonschemars` codegen
/// requires a root schema of `type: "object"` with properties (see design.md, `type: "array"`).
fn large_array(rng: &mut SplitMix64, record_count: usize) -> Fixture {
    let record_schema: Value = json!({
        "type": "object",
        "properties": {
            "id": {"type": "integer", "minimum": 0, "maximum": 1_000_000_000},
            "name": {"type": "string", "minLength": 1, "maxLength": 32},
            "status": {"type": "string", "enum": RECORD_STATUSES},
            "priority": {"type": "integer", "minimum": 1, "maximum": 5},
            "score": {"type": "number", "minimum": 0, "maximum": 100},
            "verified": {"type": "boolean"},
            "tags": {
                "type": "array",
                "items": {"type": "string", "minLength": 1, "maxLength": 16},
                "maxItems": 8,
            },
            "address": {
                "type": "object",
                "properties": {
                    "street": {"type": "string", "maxLength": 64},
                    "city": {"type": "string", "minLength": 1, "maxLength": 32},
                    "postal_code": {"type": "string", "minLength": 5, "maxLength": 10},
                },
                "required": ["street", "city", "postal_code"],
            },
        },
        "required": ["id", "name", "status", "priority", "address"],
    });
    let valid: Vec<Value> = (0..record_count)
        .map(|id| large_array_record(rng, id))
        .collect();
    let mut invalid: Vec<Value> = valid.clone();
    if let Some(last) = invalid.last_mut().and_then(Value::as_object_mut) {
        last.insert("priority".to_owned(), json!(6));
    }
    Fixture {
        name: "large-array",
        schema: json!({
            "$schema": DRAFT_2020_12,
            "title": "LargeArray",
            "type": "object",
            "properties": {
                "records": {"type": "array", "items": {"$ref": "#/$defs/record"}},
            },
            "required": ["records"],
            "$defs": {"record": record_schema},
        }),
        valid_file: "all-records.json",
        valid: json!({"records": valid}),
        invalid_file: "last-record-priority-above-maximum.json",
        invalid: json!({"records": invalid}),
    }
}

/// An object nested `levels` levels deep through `properties` (each level a distinct inline
/// object with two leaf fields). Built inside out with a loop so no recursion is needed. The
/// invalid instance puts the deepest level's count above its maximum.
fn deep_nesting(rng: &mut SplitMix64, levels: usize) -> Fixture {
    let mut child: Option<(String, Value, Value)> = None;
    for level in (1..=levels).rev() {
        let prefix: String = format!("level_{level:02}");
        let label_name: String = format!("{prefix}_label");
        let count_name: String = format!("{prefix}_count");
        let mut schema_properties: Vec<(String, Value)> = vec![
            (
                label_name.clone(),
                json!({"type": "string", "minLength": 1, "maxLength": 32}),
            ),
            (
                count_name.clone(),
                json!({"type": "integer", "minimum": 0, "maximum": 1000}),
            ),
        ];
        let mut value: Map<String, Value> = Map::new();
        value.insert(label_name, Value::String(rng.lowercase(1, 32)));
        value.insert(count_name, json!(rng.range_u32(0, 1000)));
        if let Some((child_name, child_schema, child_value)) = child.take() {
            schema_properties.push((child_name.clone(), child_schema));
            value.insert(child_name, child_value);
        }
        child = Some((
            prefix,
            object_schema_all_required(schema_properties),
            Value::Object(value),
        ));
    }
    let (_, mut schema, valid): (String, Value, Value) =
        child.expect("deep-nesting has at least one level");
    if let Some(root) = schema.as_object_mut() {
        root.insert("$schema".to_owned(), json!(DRAFT_2020_12));
        root.insert("title".to_owned(), json!("DeepNesting"));
    }
    let mut invalid: Value = valid.clone();
    let deepest_pointer: String = (2..=levels)
        .map(|level| format!("/level_{level:02}"))
        .chain(std::iter::once(format!("/level_{levels:02}_count")))
        .collect();
    if let Some(count) = invalid.pointer_mut(&deepest_pointer) {
        *count = json!(1001);
    }
    Fixture {
        name: "deep-nesting",
        schema,
        valid_file: "fully-populated.json",
        valid,
        invalid_file: "deepest-count-above-maximum.json",
        invalid,
    }
}

/// Name of the `many-defs` def at `layer`, position `position`.
fn many_defs_name(layer: usize, position: usize, per_layer: usize) -> String {
    format!("def_{:03}", layer * per_layer + position)
}

/// `layers * per_layer` distinct object defs arranged as a layered DAG: each def has two leaf
/// fields and, outside the last layer, two required `$ref`s into the next layer (`child_a` at
/// the same position, so every def is referenced, and `child_b` at a random position). The root
/// references `root_entries` first-layer defs. The invalid instance puts the deepest count on
/// the last entry's `child_b` path above its maximum.
fn many_defs(
    rng: &mut SplitMix64,
    layers: usize,
    per_layer: usize,
    root_entries: usize,
) -> Fixture {
    let def_count: usize = layers * per_layer;
    let mut defs: Vec<(String, Value)> = Vec::with_capacity(def_count);
    let mut children: Vec<Option<(usize, usize)>> = Vec::with_capacity(def_count);
    for layer in 0..layers {
        for position in 0..per_layer {
            let mut properties: Vec<(String, Value)> = vec![
                (
                    "label".to_owned(),
                    json!({"type": "string", "minLength": 1, "maxLength": 24}),
                ),
                (
                    "count".to_owned(),
                    json!({"type": "integer", "minimum": 0, "maximum": 999}),
                ),
            ];
            if layer + 1 < layers {
                let child_a: usize = (layer + 1) * per_layer + position;
                let child_b: usize = (layer + 1) * per_layer + rng.index(per_layer);
                for (field, target) in [("child_a", child_a), ("child_b", child_b)] {
                    properties.push((
                        field.to_owned(),
                        json!({"$ref": format!("#/$defs/def_{target:03}")}),
                    ));
                }
                children.push(Some((child_a, child_b)));
            } else {
                children.push(None);
            }
            defs.push((
                many_defs_name(layer, position, per_layer),
                object_schema_all_required(properties),
            ));
        }
    }

    // Instances are built bottom-up (children always have a larger index), so each def's
    // instance is assembled from already-built child instances without recursion.
    let mut instances: Vec<Value> = vec![Value::Null; def_count];
    for index in (0..def_count).rev() {
        let mut value: Map<String, Value> = Map::new();
        value.insert("label".to_owned(), Value::String(rng.lowercase(1, 24)));
        value.insert("count".to_owned(), json!(rng.range_u32(0, 999)));
        if let Some((child_a, child_b)) = children[index] {
            value.insert("child_a".to_owned(), instances[child_a].clone());
            value.insert("child_b".to_owned(), instances[child_b].clone());
        }
        instances[index] = Value::Object(value);
    }

    let mut root_properties: Vec<(String, Value)> = Vec::with_capacity(root_entries);
    let mut valid: Map<String, Value> = Map::new();
    for entry in 0..root_entries {
        let position: usize = rng.index(per_layer);
        let entry_name: String = format!("entry_{entry}");
        root_properties.push((
            entry_name.clone(),
            json!({"$ref": format!("#/$defs/{}", many_defs_name(0, position, per_layer))}),
        ));
        valid.insert(entry_name, instances[position].clone());
    }
    let mut schema: Value = object_schema_all_required(root_properties);
    let defs_map: Map<String, Value> = defs.into_iter().collect();
    if let Some(root) = schema.as_object_mut() {
        root.insert("$schema".to_owned(), json!(DRAFT_2020_12));
        root.insert("title".to_owned(), json!("ManyDefs"));
        root.insert("$defs".to_owned(), Value::Object(defs_map));
    }
    let valid: Value = Value::Object(valid);
    let mut invalid: Value = valid.clone();
    if root_entries > 0 {
        let deepest_pointer: String = std::iter::once(format!("/entry_{}", root_entries - 1))
            .chain((1..layers).map(|_| "/child_b".to_owned()))
            .chain(std::iter::once("/count".to_owned()))
            .collect();
        if let Some(count) = invalid.pointer_mut(&deepest_pointer) {
            *count = json!(1000);
        }
    }
    Fixture {
        name: "many-defs",
        schema,
        valid_file: "full-graph.json",
        valid,
        invalid_file: "deepest-count-above-maximum.json",
        invalid,
    }
}

/// Every generated fixture, rendered, in a fixed order.
fn render_all() -> Vec<RenderedFixture> {
    let mut rng: SplitMix64 = SplitMix64::new(SEED);
    let fixtures: Vec<Fixture> = vec![
        wide_object(&mut rng, WIDE_OBJECT_PROPERTIES),
        large_array(&mut rng, LARGE_ARRAY_RECORDS),
        deep_nesting(&mut rng, DEEP_NESTING_LEVELS),
        many_defs(
            &mut rng,
            MANY_DEFS_LAYERS,
            MANY_DEFS_PER_LAYER,
            MANY_DEFS_ROOT_ENTRIES,
        ),
    ];
    fixtures.iter().map(Fixture::render).collect()
}

/// Writes `content` to `path` only when the file is missing or its bytes differ.
fn write_if_changed(path: &Path, content: &str) -> Result<(), String> {
    if fs::read(path).is_ok_and(|existing| existing == content.as_bytes()) {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
    }
    fs::write(path, content).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

/// Removes every file under `root` not in `keep`, then every directory left empty. Walks the
/// tree with an explicit stack; symlinks are treated as files and never followed.
fn prune(root: &Path, keep: &BTreeSet<PathBuf>) -> Result<(), String> {
    let mut stack: Vec<PathBuf> = vec![root.to_path_buf()];
    let mut directories: Vec<PathBuf> = Vec::new();
    while let Some(dir) = stack.pop() {
        let entries: fs::ReadDir =
            fs::read_dir(&dir).map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
        for entry in entries {
            let entry: fs::DirEntry =
                entry.map_err(|e| format!("cannot read entry in {}: {e}", dir.display()))?;
            let path: PathBuf = entry.path();
            let file_type: fs::FileType = entry
                .file_type()
                .map_err(|e| format!("cannot stat {}: {e}", path.display()))?;
            if file_type.is_dir() {
                directories.push(path.clone());
                stack.push(path);
            } else if !keep.contains(&path) {
                fs::remove_file(&path)
                    .map_err(|e| format!("cannot remove {}: {e}", path.display()))?;
            }
        }
    }
    // Deepest directories first, so a parent is checked only after its children are gone.
    directories.sort_by_key(|dir| std::cmp::Reverse(dir.components().count()));
    for dir in directories {
        let is_empty: bool = fs::read_dir(&dir)
            .map_err(|e| format!("cannot read {}: {e}", dir.display()))?
            .next()
            .is_none();
        if is_empty {
            fs::remove_dir(&dir).map_err(|e| format!("cannot remove {}: {e}", dir.display()))?;
        }
    }
    Ok(())
}

/// Generates every fixture under `<output>/generated/`, prunes stale files, and prints a
/// one-line summary per fixture.
fn run(output: &Path) -> Result<(), String> {
    let generated_root: PathBuf = output.join("generated");
    let rendered: Vec<RenderedFixture> = render_all();
    let mut keep: BTreeSet<PathBuf> = BTreeSet::new();
    for fixture in &rendered {
        for (relative, content) in &fixture.files {
            let path: PathBuf = generated_root.join(relative);
            write_if_changed(&path, content)?;
            keep.insert(path);
        }
    }
    prune(&generated_root, &keep)?;
    for fixture in &rendered {
        println!(
            "generated/{}: schema {} bytes, instances {} bytes",
            fixture.name, fixture.schema_bytes, fixture.instance_bytes
        );
    }
    Ok(())
}

fn main() -> ExitCode {
    let matches = Command::new("gen_fixtures")
        .about("Generate the large synthetic benchmark fixtures deterministically")
        .arg(
            Arg::new("output")
                .short('o')
                .long("output")
                .value_name("DIR")
                .required(true)
                .help("Directory under which generated/<name>/ fixtures are written"),
        )
        .get_matches();
    let output: PathBuf = matches
        .get_one::<String>("output")
        .map(PathBuf::from)
        .expect("required --output");
    match run(&output) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("gen_fixtures: {err}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{SplitMix64, deep_nesting, wide_object};
    use serde_json::{Value, json};

    #[test]
    fn splitmix64_matches_reference_sequence_for_seed_zero() {
        let mut rng: SplitMix64 = SplitMix64::new(0);
        let expected: Vec<u64> = vec![
            0xE220_A839_7B1D_CDAF,
            0x6E78_9E6A_A1B9_65F4,
            0x06C4_5D18_8009_454F,
        ];
        let actual: Vec<u64> = (0..3).map(|_| rng.next_u64()).collect();
        assert_eq!(expected, actual);
    }

    #[test]
    fn wide_object_invalid_differs_from_valid_only_in_last_field() {
        let fixture = wide_object(&mut SplitMix64::new(7), 5);
        let mut expected: Value = fixture.valid.clone();
        expected["field_0004"] = json!(101);
        let actual: Value = fixture.invalid;
        assert_eq!(expected, actual);
    }

    #[test]
    fn deep_nesting_with_two_levels_has_exact_schema() {
        let fixture = deep_nesting(&mut SplitMix64::new(7), 2);
        let expected: Value = json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "title": "DeepNesting",
            "type": "object",
            "properties": {
                "level_01_label": {"type": "string", "minLength": 1, "maxLength": 32},
                "level_01_count": {"type": "integer", "minimum": 0, "maximum": 1000},
                "level_02": {
                    "type": "object",
                    "properties": {
                        "level_02_label": {"type": "string", "minLength": 1, "maxLength": 32},
                        "level_02_count": {"type": "integer", "minimum": 0, "maximum": 1000},
                    },
                    "required": ["level_02_label", "level_02_count"],
                },
            },
            "required": ["level_01_label", "level_01_count", "level_02"],
        });
        let actual: Value = fixture.schema;
        assert_eq!(expected, actual);
    }

    #[test]
    fn deep_nesting_invalid_differs_from_valid_only_in_deepest_count() {
        let fixture = deep_nesting(&mut SplitMix64::new(7), 3);
        let mut expected: Value = fixture.valid.clone();
        expected["level_02"]["level_03"]["level_03_count"] = json!(1001);
        let actual: Value = fixture.invalid;
        assert_eq!(expected, actual);
    }
}
