//! Shared data model for the json-schema-rs benchmark harness.
//!
//! The harness compares our library against competitor tools on the same fixtures across
//! two axes (codegen and validation). Cross-tool wall-time and peak memory both come from
//! `/usr/bin/time`, while our in-process hot paths come from criterion. The aggregator parses
//! those raw outputs into the [`BenchmarkResults`] model defined here, computes a winner per
//! metric category, and renders both machine-readable JSON and a human-readable table.
//!
//! For every metric we currently capture, **lower is better** (times and peak memory), so
//! winner selection is a single "smallest value wins, within a tie threshold" rule.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

/// Relative gap (as a fraction) within which two measurements are treated as a tie.
///
/// If the best and next-best measurements for a cell are within this fraction of each
/// other, neither is declared the sole winner and the cell is a tie.
pub const TIE_THRESHOLD: f64 = 0.03;

/// A measured metric category. Lower is better for every category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    /// End-to-end wall time to generate code from a schema (codegen axis).
    CodegenWallTimeSecs,
    /// In-process time to parse and compile a schema before any instance is validated.
    SchemaCompileTimeSecs,
    /// Per-instance validation time on instances the schema accepts (valid path).
    ValidateValidTimeSecs,
    /// Per-instance validation time on instances the schema rejects (error path).
    ValidateInvalidTimeSecs,
    /// Peak resident set size observed while running the tool.
    PeakMemoryBytes,
}

impl Category {
    /// Human-readable label for tables and reports.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::CodegenWallTimeSecs => "codegen wall-time",
            Self::SchemaCompileTimeSecs => "schema-compile time",
            Self::ValidateValidTimeSecs => "validate (valid)",
            Self::ValidateInvalidTimeSecs => "validate (invalid)",
            Self::PeakMemoryBytes => "peak memory",
        }
    }

    /// Unit suffix for rendered values.
    #[must_use]
    pub const fn unit(self) -> &'static str {
        match self {
            Self::CodegenWallTimeSecs
            | Self::SchemaCompileTimeSecs
            | Self::ValidateValidTimeSecs
            | Self::ValidateInvalidTimeSecs => "s",
            Self::PeakMemoryBytes => "bytes",
        }
    }

    /// Every category, in stable reporting order.
    #[must_use]
    pub const fn all() -> [Self; 5] {
        [
            Self::CodegenWallTimeSecs,
            Self::SchemaCompileTimeSecs,
            Self::ValidateValidTimeSecs,
            Self::ValidateInvalidTimeSecs,
            Self::PeakMemoryBytes,
        ]
    }
}

/// A single measurement: one tool, one fixture, one category, one value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Measurement {
    /// Tool identifier (e.g. `"jsonschemars"`, `"typify"`, `"boon"`).
    pub tool: String,
    /// Fixture identifier (e.g. `"small/config"`, `"massive/kubernetes"`).
    pub fixture: String,
    /// Which metric was measured.
    pub category: Category,
    /// The measured value, in the category's [`Category::unit`].
    pub value: f64,
}

/// Outcome of attempting codegen for a (tool, fixture) pair.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "status", content = "detail")]
pub enum CompatStatus {
    /// The tool produced output for the fixture.
    Pass,
    /// The tool could not handle the fixture; the string is a short reason.
    Fail(String),
}

/// One entry in the codegen capability/compat matrix.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompatEntry {
    /// Tool identifier.
    pub tool: String,
    /// Fixture identifier.
    pub fixture: String,
    /// Whether the tool handled the fixture.
    pub status: CompatStatus,
}

/// The full aggregated results of a harness run.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BenchmarkResults {
    /// Every timed/memory measurement collected across tools and fixtures.
    pub measurements: Vec<Measurement>,
    /// The codegen pass/fail matrix (not timed).
    pub compat: Vec<CompatEntry>,
    /// Output-determinism matrix: whether each tool produced byte-identical codegen across
    /// two runs of the same fixture. Reuses [`CompatStatus`] (`Pass` = deterministic).
    #[serde(default)]
    pub determinism: Vec<CompatEntry>,
    /// Peak heap bytes for our library across the criterion hot paths, from dhat (our-lib
    /// only; competitors are black-box CLIs). `None` when the dhat profile was not run.
    #[serde(default)]
    pub heap_peak_bytes: Option<u64>,
}

/// The winner of a single (category, fixture) cell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CellWinner {
    /// Exactly one tool won this cell.
    Tool(String),
    /// The top tools were within [`TIE_THRESHOLD`]; all tied tools are listed.
    Tie(Vec<String>),
}

impl BenchmarkResults {
    /// Determine the winner for each (category, fixture) cell.
    ///
    /// Within a cell the smallest value wins; if the best and next-best are within
    /// [`TIE_THRESHOLD`] the cell is a [`CellWinner::Tie`]. Cells with a single competitor
    /// still yield a [`CellWinner::Tool`].
    #[must_use]
    pub fn cell_winners(&self) -> BTreeMap<(Category, String), CellWinner> {
        // Group measurements by (category, fixture) deterministically.
        let mut cells: BTreeMap<(Category, String), Vec<(&str, f64)>> = BTreeMap::new();
        for m in &self.measurements {
            cells
                .entry((m.category, m.fixture.clone()))
                .or_default()
                .push((m.tool.as_str(), m.value));
        }

        let mut winners: BTreeMap<(Category, String), CellWinner> = BTreeMap::new();
        for (key, mut entries) in cells {
            // Sort by value ascending, then tool name for determinism on exact ties.
            entries.sort_by(|a, b| {
                a.1.partial_cmp(&b.1)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.0.cmp(b.0))
            });
            let best: f64 = entries[0].1;
            // Collect every tool within the tie threshold of the best value.
            let tied: Vec<String> = entries
                .iter()
                .filter(|(_, v)| within_tie(best, *v))
                .map(|(t, _)| (*t).to_owned())
                .collect();
            let winner: CellWinner = if tied.len() > 1 {
                CellWinner::Tie(tied)
            } else {
                CellWinner::Tool(entries[0].0.to_owned())
            };
            winners.insert(key, winner);
        }
        winners
    }

    /// Tally per-category champions by counting how many cells each tool won outright.
    ///
    /// Ties award a fractional point to nobody (only outright wins count), so the champion
    /// is the tool that most often beat everyone else cleanly on that metric.
    #[must_use]
    pub fn category_champions(&self) -> BTreeMap<Category, Option<String>> {
        let cells = self.cell_winners();
        let mut wins: BTreeMap<Category, BTreeMap<String, u32>> = BTreeMap::new();
        for ((category, _fixture), winner) in cells {
            if let CellWinner::Tool(tool) = winner {
                *wins.entry(category).or_default().entry(tool).or_insert(0) += 1;
            }
        }

        let mut champions: BTreeMap<Category, Option<String>> = BTreeMap::new();
        for category in Category::all() {
            let champion: Option<String> = wins.get(&category).and_then(|tally| {
                tally
                    .iter()
                    .max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0)))
                    .map(|(tool, _)| tool.clone())
            });
            champions.insert(category, champion);
        }
        champions
    }

    /// Render a human-readable Markdown report: per-category champions, a detailed
    /// fixture × tool table per category (winning cell **bold**), and the codegen compat
    /// matrix. Written to `results.md` so a PR reviewer sees the performance impact in-diff.
    #[must_use]
    pub fn to_markdown(&self) -> String {
        let mut out: String = String::new();
        out.push_str("# Benchmark results\n\n");

        // Champions: the tool that wins the most cells outright in each category.
        out.push_str("## Category winners\n\n");
        out.push_str(
            "> Winners count outright per-cell wins only. Time categories also include our \
             in-process criterion cells (our library only), and many cross-tool CLI cells \
             round to `0.000` under `/usr/bin/time`'s ~10 ms resolution (counted as ties, \
             not wins), so a time-category winner can reflect criterion-only cells rather \
             than head-to-head speed. Peak memory is measured head-to-head across tools. \
             See issue #21.\n\n",
        );
        out.push_str("| Category | Winner |\n|---|---|\n");
        for (category, champion) in self.category_champions() {
            let winner: &str = champion.as_deref().unwrap_or("(no data)");
            // `write!` to a String is infallible.
            let _ = writeln!(out, "| {} | {winner} |", category.label());
        }

        // Detailed per-category tables with the winning cell bolded.
        let winners: BTreeMap<(Category, String), CellWinner> = self.cell_winners();
        out.push_str("\n## Detailed results\n");
        for category in Category::all() {
            let rows: Vec<&Measurement> = self
                .measurements
                .iter()
                .filter(|m| m.category == category)
                .collect();
            if rows.is_empty() {
                continue;
            }
            let tools: BTreeSet<&str> = rows.iter().map(|m| m.tool.as_str()).collect();
            let fixtures: BTreeSet<&str> = rows.iter().map(|m| m.fixture.as_str()).collect();

            let _ = write!(
                out,
                "\n### {} ({})\n\n| Fixture |",
                category.label(),
                value_units(category)
            );
            for tool in &tools {
                let _ = write!(out, " {tool} |");
            }
            out.push_str("\n|---|");
            for _ in &tools {
                out.push_str("---|");
            }
            out.push('\n');

            for fixture in &fixtures {
                let _ = write!(out, "| {fixture} |");
                let winner: Option<&CellWinner> = winners.get(&(category, (*fixture).to_owned()));
                for tool in &tools {
                    match rows
                        .iter()
                        .find(|m| m.fixture == *fixture && m.tool == *tool)
                    {
                        Some(m) => {
                            let cell: String = format_value(category, m.value);
                            if is_winner(winner, tool) {
                                let _ = write!(out, " **{cell}** |");
                            } else {
                                let _ = write!(out, " {cell} |");
                            }
                        }
                        None => out.push_str(" — |"),
                    }
                }
                out.push('\n');
            }
        }

        out.push_str("\n## Codegen compatibility\n\n");
        out.push_str("| Tool | Fixture | Status |\n|---|---|---|\n");
        for entry in &self.compat {
            let status: String = match &entry.status {
                CompatStatus::Pass => "pass".to_owned(),
                CompatStatus::Fail(reason) => format!("FAIL: {reason}"),
            };
            let _ = writeln!(out, "| {} | {} | {status} |", entry.tool, entry.fixture);
        }

        // Output determinism: is each tool's codegen byte-identical across two runs?
        if !self.determinism.is_empty() {
            out.push_str("\n## Output determinism\n\n");
            out.push_str("| Tool | Fixture | Deterministic |\n|---|---|---|\n");
            for entry in &self.determinism {
                let status: String = match &entry.status {
                    CompatStatus::Pass => "yes".to_owned(),
                    CompatStatus::Fail(reason) => format!("NO: {reason}"),
                };
                let _ = writeln!(out, "| {} | {} | {status} |", entry.tool, entry.fixture);
            }
        }

        // Heap profile (our library only, via dhat).
        if let Some(bytes) = self.heap_peak_bytes {
            let _ = write!(
                out,
                "\n## Heap profile (our lib, dhat)\n\nPeak heap: {bytes} bytes\n"
            );
        }

        out
    }
}

/// Unit description shown in a detailed-results section header for `category`.
fn value_units(category: Category) -> &'static str {
    match category {
        Category::PeakMemoryBytes => "bytes; lower is better",
        _ => "microseconds; lower is better",
    }
}

/// Format a measured value for display in the given category's unit.
fn format_value(category: Category, value: f64) -> String {
    match category {
        Category::PeakMemoryBytes => format!("{value:.0}"),
        // Time categories are stored in seconds; show microseconds for readability across
        // both criterion (sub-µs) and CLI (ms) magnitudes.
        _ => format!("{:.3}", value * 1e6),
    }
}

/// Whether `tool` is (one of) the winner(s) of a cell.
fn is_winner(winner: Option<&CellWinner>, tool: &str) -> bool {
    match winner {
        Some(CellWinner::Tool(w)) => w == tool,
        Some(CellWinner::Tie(tied)) => tied.iter().any(|t| t == tool),
        None => false,
    }
}

/// True when `value` is within [`TIE_THRESHOLD`] of `best` (the smaller, better value).
fn within_tie(best: f64, value: f64) -> bool {
    if best <= 0.0 {
        return (value - best).abs() <= f64::EPSILON;
    }
    (value - best) / best <= TIE_THRESHOLD
}

/// Parse a `snake_case` category token (as written by the driver) into a [`Category`].
///
/// Reuses the serde rename mapping so the token vocabulary has a single source of truth.
#[must_use]
pub fn parse_category(token: &str) -> Option<Category> {
    serde_json::from_value(serde_json::Value::String(token.to_owned())).ok()
}

/// Parse the driver's tab-separated CLI measurement capture.
///
/// Each line is `tool\tfixture\tcategory_token\tvalue`. Blank lines, lines beginning with
/// `#`, malformed lines, unknown categories, and unparsable values are skipped so a partial
/// or tool-absent run still aggregates cleanly.
#[must_use]
pub fn parse_cli_measurements(text: &str) -> Vec<Measurement> {
    let mut out: Vec<Measurement> = Vec::new();
    for line in text.lines() {
        let line: &str = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        let [tool, fixture, category_token, value] = fields.as_slice() else {
            continue;
        };
        let (Some(category), Ok(value)) = (parse_category(category_token), value.parse::<f64>())
        else {
            continue;
        };
        out.push(Measurement {
            tool: (*tool).to_owned(),
            fixture: (*fixture).to_owned(),
            category,
            value,
        });
    }
    out
}

/// Parse the driver's tab-separated codegen compat capture.
///
/// Each line is `tool\tfixture\tstatus\tdetail`, where `status` is `pass` or `fail` and
/// `detail` (the failure reason) may be empty. Unknown statuses and malformed lines are
/// skipped.
#[must_use]
pub fn parse_compat(text: &str) -> Vec<CompatEntry> {
    let mut out: Vec<CompatEntry> = Vec::new();
    for line in text.lines() {
        let line: &str = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // Detail may contain no trailing field; split into at most 4 parts.
        let mut fields = line.splitn(4, '\t');
        let (Some(tool), Some(fixture), Some(status)) =
            (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        let detail: &str = fields.next().unwrap_or("");
        let status: CompatStatus = match status {
            "pass" => CompatStatus::Pass,
            "fail" => CompatStatus::Fail(if detail.is_empty() {
                "unsupported".to_owned()
            } else {
                detail.to_owned()
            }),
            _ => continue,
        };
        out.push(CompatEntry {
            tool: tool.to_owned(),
            fixture: fixture.to_owned(),
            status,
        });
    }
    out
}

/// Extract a point estimate, in **seconds**, from a criterion `estimates.json` document.
///
/// criterion reports nanoseconds; we prefer the median (robust to outliers) and fall back
/// to the mean. Returns `None` if the document lacks a usable estimate.
#[must_use]
pub fn parse_criterion_estimate_secs(json: &str) -> Option<f64> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let nanos: f64 = value
        .get("median")
        .or_else(|| value.get("mean"))
        .and_then(|est| est.get("point_estimate"))
        .and_then(serde_json::Value::as_f64)?;
    Some(nanos / 1e9)
}

/// Collapse duplicate measurements to one value per `(tool, fixture, category)`.
///
/// A tool can be measured more than once for the same cell (e.g. peak RSS is sampled during
/// both codegen and validation). Since **lower is better**, we keep the *min* for time
/// categories (the tool's best time) and the *max* for peak memory (the true peak footprint
/// across the operations the tool performs). The result is deterministically ordered.
#[must_use]
pub fn collapse_measurements(measurements: &[Measurement]) -> Vec<Measurement> {
    let mut best: BTreeMap<(String, String, Category), f64> = BTreeMap::new();
    for m in measurements {
        let key: (String, String, Category) = (m.tool.clone(), m.fixture.clone(), m.category);
        best.entry(key)
            .and_modify(|v| {
                *v = if m.category == Category::PeakMemoryBytes {
                    v.max(m.value)
                } else {
                    v.min(m.value)
                };
            })
            .or_insert(m.value);
    }
    best.into_iter()
        .map(|((tool, fixture, category), value)| Measurement {
            tool,
            fixture,
            category,
            value,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn measurement(tool: &str, fixture: &str, category: Category, value: f64) -> Measurement {
        Measurement {
            tool: tool.to_owned(),
            fixture: fixture.to_owned(),
            category,
            value,
        }
    }

    #[test]
    fn cell_winner_picks_smallest_value() {
        let results: BenchmarkResults = BenchmarkResults {
            measurements: vec![
                measurement("ours", "small", Category::ValidateValidTimeSecs, 1.0),
                measurement("boon", "small", Category::ValidateValidTimeSecs, 2.0),
            ],
            compat: vec![],
            ..Default::default()
        };
        let expected: CellWinner = CellWinner::Tool("ours".to_owned());
        let actual: CellWinner = results
            .cell_winners()
            .remove(&(Category::ValidateValidTimeSecs, "small".to_owned()))
            .expect("cell present");
        assert_eq!(expected, actual);
    }

    #[test]
    fn cell_winner_reports_tie_within_threshold() {
        let results: BenchmarkResults = BenchmarkResults {
            measurements: vec![
                measurement("ours", "small", Category::ValidateValidTimeSecs, 1.00),
                measurement("boon", "small", Category::ValidateValidTimeSecs, 1.02),
            ],
            compat: vec![],
            ..Default::default()
        };
        let expected: CellWinner = CellWinner::Tie(vec!["ours".to_owned(), "boon".to_owned()]);
        let actual: CellWinner = results
            .cell_winners()
            .remove(&(Category::ValidateValidTimeSecs, "small".to_owned()))
            .expect("cell present");
        assert_eq!(expected, actual);
    }

    #[test]
    fn category_champion_counts_outright_wins_only() {
        let results: BenchmarkResults = BenchmarkResults {
            measurements: vec![
                measurement("ours", "a", Category::CodegenWallTimeSecs, 1.0),
                measurement("typify", "a", Category::CodegenWallTimeSecs, 2.0),
                measurement("ours", "b", Category::CodegenWallTimeSecs, 1.0),
                measurement("typify", "b", Category::CodegenWallTimeSecs, 2.0),
            ],
            compat: vec![],
            ..Default::default()
        };
        let expected: Option<String> = Some("ours".to_owned());
        let actual: Option<String> = results
            .category_champions()
            .remove(&Category::CodegenWallTimeSecs)
            .expect("category present");
        assert_eq!(expected, actual);
    }

    #[test]
    fn category_champion_is_none_when_all_cells_tie() {
        let results: BenchmarkResults = BenchmarkResults {
            measurements: vec![
                measurement("ours", "a", Category::CodegenWallTimeSecs, 1.00),
                measurement("typify", "a", Category::CodegenWallTimeSecs, 1.01),
            ],
            compat: vec![],
            ..Default::default()
        };
        let expected: Option<String> = None;
        let actual: Option<String> = results
            .category_champions()
            .remove(&Category::CodegenWallTimeSecs)
            .expect("category present");
        assert_eq!(expected, actual);
    }

    #[test]
    fn within_tie_treats_zero_best_as_exact() {
        let expected: bool = false;
        let actual: bool = within_tie(0.0, 0.5);
        assert_eq!(expected, actual);
    }

    #[test]
    fn markdown_renders_full_report() {
        let results: BenchmarkResults = BenchmarkResults {
            measurements: vec![
                measurement(
                    "jsonschemars",
                    "small/config",
                    Category::CodegenWallTimeSecs,
                    1e-5,
                ),
                measurement(
                    "typify",
                    "small/config",
                    Category::CodegenWallTimeSecs,
                    2e-5,
                ),
            ],
            compat: vec![CompatEntry {
                tool: "jsonschemars".to_owned(),
                fixture: "small/config".to_owned(),
                status: CompatStatus::Pass,
            }],
            ..Default::default()
        };
        let expected: String = "\
# Benchmark results

## Category winners

> Winners count outright per-cell wins only. Time categories also include our in-process criterion cells (our library only), and many cross-tool CLI cells round to `0.000` under `/usr/bin/time`'s ~10 ms resolution (counted as ties, not wins), so a time-category winner can reflect criterion-only cells rather than head-to-head speed. Peak memory is measured head-to-head across tools. See issue #21.

| Category | Winner |
|---|---|
| codegen wall-time | jsonschemars |
| schema-compile time | (no data) |
| validate (valid) | (no data) |
| validate (invalid) | (no data) |
| peak memory | (no data) |

## Detailed results

### codegen wall-time (microseconds; lower is better)

| Fixture | jsonschemars | typify |
|---|---|---|
| small/config | **10.000** | 20.000 |

## Codegen compatibility

| Tool | Fixture | Status |
|---|---|---|
| jsonschemars | small/config | pass |
"
        .to_owned();
        let actual: String = results.to_markdown();
        assert_eq!(expected, actual);
    }

    #[test]
    fn markdown_renders_determinism_and_heap_sections() {
        let results: BenchmarkResults = BenchmarkResults {
            determinism: vec![CompatEntry {
                tool: "typify".to_owned(),
                fixture: "small/config".to_owned(),
                status: CompatStatus::Fail("differs".to_owned()),
            }],
            heap_peak_bytes: Some(12345),
            ..Default::default()
        };
        let expected: String = "\
# Benchmark results

## Category winners

> Winners count outright per-cell wins only. Time categories also include our in-process criterion cells (our library only), and many cross-tool CLI cells round to `0.000` under `/usr/bin/time`'s ~10 ms resolution (counted as ties, not wins), so a time-category winner can reflect criterion-only cells rather than head-to-head speed. Peak memory is measured head-to-head across tools. See issue #21.

| Category | Winner |
|---|---|
| codegen wall-time | (no data) |
| schema-compile time | (no data) |
| validate (valid) | (no data) |
| validate (invalid) | (no data) |
| peak memory | (no data) |

## Detailed results

## Codegen compatibility

| Tool | Fixture | Status |
|---|---|---|

## Output determinism

| Tool | Fixture | Deterministic |
|---|---|---|
| typify | small/config | NO: differs |

## Heap profile (our lib, dhat)

Peak heap: 12345 bytes
"
        .to_owned();
        let actual: String = results.to_markdown();
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_category_maps_snake_case_token() {
        let expected: Option<Category> = Some(Category::PeakMemoryBytes);
        let actual: Option<Category> = parse_category("peak_memory_bytes");
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_category_rejects_unknown_token() {
        let expected: Option<Category> = None;
        let actual: Option<Category> = parse_category("not_a_category");
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_cli_measurements_reads_valid_lines_and_skips_junk() {
        let text: &str = "# header\n\
            jsonschemars\tsmall/config\tcodegen_wall_time_secs\t0.012\n\
            \n\
            boon\tsmall/config\tbad_category\t0.5\n\
            typify\tsmall/config\tpeak_memory_bytes\tnot_a_number\n\
            boon\tsmall/config\tvalidate_valid_time_secs\t0.004\n";
        let expected: Vec<Measurement> = vec![
            Measurement {
                tool: "jsonschemars".to_owned(),
                fixture: "small/config".to_owned(),
                category: Category::CodegenWallTimeSecs,
                value: 0.012,
            },
            Measurement {
                tool: "boon".to_owned(),
                fixture: "small/config".to_owned(),
                category: Category::ValidateValidTimeSecs,
                value: 0.004,
            },
        ];
        let actual: Vec<Measurement> = parse_cli_measurements(text);
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_compat_reads_pass_and_fail_with_detail() {
        let text: &str = "typify\tsmall/config\tpass\t\n\
            schemafy\tlarge/azure\tfail\tno CLI available\n\
            junk\tline\tbogus\tx\n";
        let expected: Vec<CompatEntry> = vec![
            CompatEntry {
                tool: "typify".to_owned(),
                fixture: "small/config".to_owned(),
                status: CompatStatus::Pass,
            },
            CompatEntry {
                tool: "schemafy".to_owned(),
                fixture: "large/azure".to_owned(),
                status: CompatStatus::Fail("no CLI available".to_owned()),
            },
        ];
        let actual: Vec<CompatEntry> = parse_compat(text);
        assert_eq!(expected, actual);
    }

    #[test]
    fn collapse_keeps_max_memory_and_min_time() {
        let input: Vec<Measurement> = vec![
            measurement("ours", "f", Category::PeakMemoryBytes, 4.0),
            measurement("ours", "f", Category::PeakMemoryBytes, 12.0),
            measurement("ours", "f", Category::ValidateValidTimeSecs, 0.9),
            measurement("ours", "f", Category::ValidateValidTimeSecs, 0.3),
        ];
        let expected: Vec<Measurement> = vec![
            measurement("ours", "f", Category::ValidateValidTimeSecs, 0.3),
            measurement("ours", "f", Category::PeakMemoryBytes, 12.0),
        ];
        let actual: Vec<Measurement> = collapse_measurements(&input);
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_criterion_estimate_prefers_median_in_seconds() {
        let json: &str =
            r#"{"mean":{"point_estimate":2000000000.0},"median":{"point_estimate":1000000000.0}}"#;
        let expected: Option<f64> = Some(1.0);
        let actual: Option<f64> = parse_criterion_estimate_secs(json);
        assert_eq!(expected, actual);
    }
}
