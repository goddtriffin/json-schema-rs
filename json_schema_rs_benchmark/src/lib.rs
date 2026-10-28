//! Shared results model for the json-schema-rs benchmark harness.
//!
//! The harness compares our library against competitor tools on the same fixtures in two
//! tiers:
//!
//! - **In-process** ([`Tier::InProcess`]): every tool is linked as a Rust library into one
//!   process. criterion times each operation (median and standard deviation per call) and a
//!   counting allocator records the peak heap each operation allocates.
//! - **CLI** ([`Tier::Cli`]): every tool's command-line binary runs end to end, process
//!   startup included. hyperfine times it (warmup plus repeated runs) and records the peak
//!   resident set size of each run.
//!
//! This module holds the data model ([`BenchmarkResults`]), pure parsers for every raw
//! capture format (criterion JSON, hyperfine export JSON, and the harness's tab-separated
//! files), the statistical winner rule ([`decide_cell`]), and the Markdown renderer
//! ([`BenchmarkResults::to_markdown`]). File I/O lives in the `aggregate` binary.
//!
//! For every metric, **lower is better** (time and peak memory). Every ordering is explicit
//! and total, so the same inputs always render byte-identical `results.json` and `results.md`.

pub mod adapters;
pub mod fixtures;

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

/// Minimum relative lead (fraction of the best median) a tool needs to win a cell outright.
pub const TIE_THRESHOLD: f64 = 0.03;

/// Standard deviation / median ratio above which a measurement is flagged low-confidence.
pub const LOW_CONFIDENCE_RATIO: f64 = 0.10;

/// Marker appended to a low-confidence measurement in the Markdown report.
pub const LOW_CONFIDENCE_MARK: &str = "†";

/// Rendered in a table cell that has no measurement.
const MISSING_CELL: &str = "—";

/// Bytes in one KiB.
const KIB: f64 = 1024.0;

/// Bytes in one MiB.
const MIB: f64 = 1024.0 * 1024.0;

/// How a tool was driven while being measured.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    /// Every tool linked as a Rust library into one process (criterion + counting allocator).
    InProcess,
    /// Every tool's CLI binary run end to end, process startup included (hyperfine).
    Cli,
}

impl Tier {
    /// Every tier, in reporting order.
    pub const ALL: [Self; 2] = [Self::InProcess, Self::Cli];

    /// Human-readable label for tables.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::InProcess => "in-process",
            Self::Cli => "CLI",
        }
    }

    /// Section heading for this tier's detailed results.
    #[must_use]
    pub const fn heading(self) -> &'static str {
        match self {
            Self::InProcess => "In-process (library calls)",
            Self::Cli => "CLI (end to end)",
        }
    }
}

/// The operation a measurement or compatibility entry covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    /// Schema JSON text to Rust source text.
    Codegen,
    /// Schema JSON text to a compiled, ready-to-validate schema.
    SchemaCompile,
    /// Validating the fixture's valid instance against the compiled schema.
    ValidateValid,
    /// Validating the fixture's invalid instance against the compiled schema.
    ValidateInvalid,
}

impl Operation {
    /// Every operation, in reporting order.
    pub const ALL: [Self; 4] = [
        Self::Codegen,
        Self::SchemaCompile,
        Self::ValidateValid,
        Self::ValidateInvalid,
    ];

    /// The `snake_case` token used in raw captures and `results.json`.
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::Codegen => "codegen",
            Self::SchemaCompile => "schema_compile",
            Self::ValidateValid => "validate_valid",
            Self::ValidateInvalid => "validate_invalid",
        }
    }

    /// Parse a `snake_case` token; `None` for an unknown token.
    #[must_use]
    pub fn from_token(token: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|operation| operation.token() == token)
    }

    /// Human-readable label for tables.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Codegen => "codegen",
            Self::SchemaCompile => "schema compile",
            Self::ValidateValid => "validate (valid)",
            Self::ValidateInvalid => "validate (invalid)",
        }
    }
}

/// What was measured. Lower is better for every metric.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Metric {
    /// Time in seconds (per call in-process; per process run on the CLI tier).
    TimeSecs,
    /// Peak memory in bytes (extra peak heap in-process; peak RSS on the CLI tier).
    PeakMemoryBytes,
}

impl Metric {
    /// Every metric, in reporting order.
    pub const ALL: [Self; 2] = [Self::TimeSecs, Self::PeakMemoryBytes];

    /// Human-readable label for tables; memory is named for what each tier records.
    #[must_use]
    pub const fn label(self, tier: Tier) -> &'static str {
        match (self, tier) {
            (Self::TimeSecs, _) => "time",
            (Self::PeakMemoryBytes, Tier::InProcess) => "peak heap",
            (Self::PeakMemoryBytes, Tier::Cli) => "peak RSS",
        }
    }
}

/// Summary statistics of one measurement, in the metric's unit (seconds or bytes).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Stats {
    /// Median of the samples (criterion's median estimate for in-process time).
    pub median: f64,
    /// Sample standard deviation (criterion's estimate for in-process time; 0 for one sample).
    pub stddev: f64,
    /// Smallest sample.
    pub min: f64,
    /// Largest sample.
    pub max: f64,
    /// Number of samples the statistics summarize.
    pub samples: u64,
}

impl Stats {
    /// Statistics of a single exact value (one sample, zero spread).
    #[must_use]
    pub const fn single(value: f64) -> Self {
        Self {
            median: value,
            stddev: 0.0,
            min: value,
            max: value,
            samples: 1,
        }
    }

    /// Statistics of a sample set: median, sample standard deviation (`n - 1` denominator,
    /// 0 for one sample), min, max, and count. `None` when `samples` is empty.
    #[must_use]
    pub fn from_samples(samples: &[f64]) -> Option<Self> {
        let mut sorted: Vec<f64> = samples.to_vec();
        sorted.sort_by(f64::total_cmp);
        let count: usize = sorted.len();
        let min: f64 = *sorted.first()?;
        let max: f64 = *sorted.last()?;
        let median: f64 = if count % 2 == 1 {
            sorted[count / 2]
        } else {
            f64::midpoint(sorted[count / 2 - 1], sorted[count / 2])
        };
        let mean: f64 = sorted.iter().sum::<f64>() / count_as_f64(count);
        let stddev: f64 = if count < 2 {
            0.0
        } else {
            let squares: f64 = sorted.iter().map(|value| (value - mean).powi(2)).sum();
            (squares / count_as_f64(count - 1)).sqrt()
        };
        Some(Self {
            median,
            stddev,
            min,
            max,
            samples: u64::try_from(count).unwrap_or(u64::MAX),
        })
    }

    /// Whether the spread is large relative to the median (`stddev / median >`
    /// [`LOW_CONFIDENCE_RATIO`]). A zero median is low-confidence only when it has any spread.
    #[must_use]
    pub fn is_low_confidence(&self) -> bool {
        if self.median <= 0.0 {
            return self.stddev > 0.0;
        }
        self.stddev / self.median > LOW_CONFIDENCE_RATIO
    }
}

/// Convert a count to `f64` for averaging (counts here are far below 2^52).
#[expect(
    clippy::cast_precision_loss,
    reason = "sample counts are far below 2^52, so the conversion is exact"
)]
const fn count_as_f64(count: usize) -> f64 {
    count as f64
}

/// Convert a byte count to `f64` (byte counts here are far below 2^52).
#[expect(
    clippy::cast_precision_loss,
    reason = "peak byte counts are far below 2^52, so the conversion is exact"
)]
const fn bytes_as_f64(bytes: u64) -> f64 {
    bytes as f64
}

/// One measured cell: a tool on a fixture for an operation and metric, in a tier.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Measurement {
    /// How the tool was driven.
    pub tier: Tier,
    /// Tool identifier (e.g. `"jsonschemars"`, `"typify"`, `"boon"`).
    pub tool: String,
    /// Fixture identifier (e.g. `"small/config"`, `"generated/wide-object"`).
    pub fixture: String,
    /// The operation measured.
    pub operation: Operation,
    /// The metric measured.
    pub metric: Metric,
    /// Summary statistics in the metric's unit (seconds or bytes).
    pub stats: Stats,
}

/// Whether a tool handled a (fixture, operation) correctly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "status", content = "detail")]
pub enum CompatStatus {
    /// The tool produced a correct result, so the cell was measured.
    Pass,
    /// The tool failed or produced a wrong verdict; the string is a short reason (may be empty).
    Fail(String),
}

/// One entry in the compatibility matrix.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompatEntry {
    /// The tier the attempt ran in.
    pub tier: Tier,
    /// Tool identifier.
    pub tool: String,
    /// Fixture identifier.
    pub fixture: String,
    /// The operation attempted.
    pub operation: Operation,
    /// Whether the tool handled it.
    pub status: CompatStatus,
}

/// The full aggregated results of a harness run (serialized as `results.json`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BenchmarkResults {
    /// Version of each tool measured (`tool -> version`); empty when not captured.
    pub tool_versions: BTreeMap<String, String>,
    /// Every measurement, in both tiers.
    pub measurements: Vec<Measurement>,
    /// Every compatibility entry, in both tiers.
    pub compat: Vec<CompatEntry>,
}

/// Identifies one comparison cell: every tool measured for this key competes.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CellKey {
    /// Tier of the cell.
    pub tier: Tier,
    /// Operation of the cell.
    pub operation: Operation,
    /// Metric of the cell.
    pub metric: Metric,
    /// Fixture of the cell.
    pub fixture: String,
}

/// Identifies one reporting category: a (tier, operation, metric) across all fixtures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CategoryKey {
    /// Tier of the category.
    pub tier: Tier,
    /// Operation of the category.
    pub operation: Operation,
    /// Metric of the category.
    pub metric: Metric,
}

/// The result of applying the winner rule to one cell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CellOutcome {
    /// Contested, and the best tool's lead beats both the tie threshold and the noise.
    Outright(String),
    /// Contested, but the best tool's lead is within the tie threshold or the noise.
    Tie,
    /// Only one tool was measured, so nothing was compared.
    Uncontested,
}

/// How strongly one category was won across its cells.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CategorySummary {
    /// Tools with the most outright wins (empty when no cell was won outright; more than one
    /// when several tools share the top count). Sorted by tool name.
    pub leaders: Vec<String>,
    /// Outright wins of each leader (0 when there are no leaders).
    pub leader_wins: u64,
    /// Number of cells in the category with at least two tools measured.
    pub contested_cells: u64,
}

/// A raw capture line that was skipped because it was malformed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedLine {
    /// 1-based line number in the source file.
    pub line: usize,
    /// Why the line was skipped.
    pub reason: String,
}

/// The rows parsed from a tab-separated capture, plus every malformed line that was skipped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TsvParse<T> {
    /// Successfully parsed rows, in file order.
    pub rows: Vec<T>,
    /// Malformed lines, in file order; the caller reports them.
    pub skipped: Vec<SkippedLine>,
}

impl<T> Default for TsvParse<T> {
    fn default() -> Self {
        Self {
            rows: Vec::new(),
            skipped: Vec::new(),
        }
    }
}

/// One row of `cli/index.tsv`: where the hyperfine export for a CLI cell lives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexEntry {
    /// Tool identifier.
    pub tool: String,
    /// Fixture identifier.
    pub fixture: String,
    /// The operation timed.
    pub operation: Operation,
    /// Path of the hyperfine `--export-json` file, relative to the raw capture directory.
    pub json_path: String,
}

/// The identity of one criterion benchmark (from its `benchmark.json`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CriterionId {
    /// The operation (criterion group id).
    pub operation: Operation,
    /// Tool identifier (criterion function id).
    pub tool: String,
    /// Fixture identifier (criterion parameter / `value_str`).
    pub fixture: String,
}

/// Decide the winner of one cell from each tool's statistics.
///
/// Entries are ranked by median (then tool name, so exact ties are deterministic). The best
/// tool wins outright only when at least two tools were measured and
/// `next.median - best.median > max(TIE_THRESHOLD * best.median, best.stddev + next.stddev)`;
/// otherwise the cell is a [`CellOutcome::Tie`] (two or more tools) or
/// [`CellOutcome::Uncontested`] (one tool). `None` for an empty cell.
#[must_use]
pub fn decide_cell(entries: &[(&str, Stats)]) -> Option<CellOutcome> {
    let mut ranked: Vec<&(&str, Stats)> = entries.iter().collect();
    ranked.sort_by(|a, b| a.1.median.total_cmp(&b.1.median).then_with(|| a.0.cmp(b.0)));
    let best: &(&str, Stats) = ranked.first()?;
    let Some(next) = ranked.get(1) else {
        return Some(CellOutcome::Uncontested);
    };
    let gap: f64 = next.1.median - best.1.median;
    let threshold: f64 = (TIE_THRESHOLD * best.1.median).max(best.1.stddev + next.1.stddev);
    if gap > threshold {
        Some(CellOutcome::Outright(best.0.to_owned()))
    } else {
        Some(CellOutcome::Tie)
    }
}

impl BenchmarkResults {
    /// Sort measurements and compat entries into their canonical order and drop duplicates
    /// (same tier, tool, fixture, operation, and metric), keeping the first occurrence.
    ///
    /// Returns one message per dropped duplicate so the caller can report it.
    pub fn normalize(&mut self) -> Vec<String> {
        let mut warnings: Vec<String> = Vec::new();

        self.measurements
            .sort_by(|a, b| measurement_order(a).cmp(&measurement_order(b)));
        let mut measurements: Vec<Measurement> = Vec::with_capacity(self.measurements.len());
        for measurement in self.measurements.drain(..) {
            let duplicate: bool = measurements.last().is_some_and(|previous: &Measurement| {
                measurement_order(previous) == measurement_order(&measurement)
            });
            if duplicate {
                warnings.push(format!(
                    "duplicate measurement dropped: {} {} {} {} {}",
                    measurement.tier.label(),
                    measurement.tool,
                    measurement.fixture,
                    measurement.operation.token(),
                    measurement.metric.label(measurement.tier),
                ));
            } else {
                measurements.push(measurement);
            }
        }
        self.measurements = measurements;

        self.compat
            .sort_by(|a, b| compat_order(a).cmp(&compat_order(b)));
        let mut compat: Vec<CompatEntry> = Vec::with_capacity(self.compat.len());
        for entry in self.compat.drain(..) {
            let duplicate: bool = compat.last().is_some_and(|previous: &CompatEntry| {
                compat_order(previous) == compat_order(&entry)
            });
            if duplicate {
                warnings.push(format!(
                    "duplicate compat entry dropped: {} {} {} {}",
                    entry.tier.label(),
                    entry.tool,
                    entry.fixture,
                    entry.operation.token(),
                ));
            } else {
                compat.push(entry);
            }
        }
        self.compat = compat;

        warnings
    }

    /// Apply [`decide_cell`] to every cell that has at least one measurement.
    #[must_use]
    pub fn cell_outcomes(&self) -> BTreeMap<CellKey, CellOutcome> {
        let mut cells: BTreeMap<CellKey, BTreeMap<&str, Stats>> = BTreeMap::new();
        for measurement in &self.measurements {
            let key: CellKey = CellKey {
                tier: measurement.tier,
                operation: measurement.operation,
                metric: measurement.metric,
                fixture: measurement.fixture.clone(),
            };
            cells
                .entry(key)
                .or_default()
                .entry(measurement.tool.as_str())
                .or_insert(measurement.stats);
        }

        let mut outcomes: BTreeMap<CellKey, CellOutcome> = BTreeMap::new();
        for (key, tools) in cells {
            let entries: Vec<(&str, Stats)> = tools.into_iter().collect();
            if let Some(outcome) = decide_cell(&entries) {
                outcomes.insert(key, outcome);
            }
        }
        outcomes
    }

    /// Tally each category: the tools with the most outright wins, and how many cells were
    /// contested. Every category with at least one measurement is present.
    #[must_use]
    pub fn category_summaries(&self) -> BTreeMap<CategoryKey, CategorySummary> {
        let mut tallies: BTreeMap<CategoryKey, (BTreeMap<String, u64>, u64)> = BTreeMap::new();
        for (cell, outcome) in self.cell_outcomes() {
            let category: CategoryKey = CategoryKey {
                tier: cell.tier,
                operation: cell.operation,
                metric: cell.metric,
            };
            let tally: &mut (BTreeMap<String, u64>, u64) = tallies.entry(category).or_default();
            match outcome {
                CellOutcome::Outright(tool) => {
                    *tally.0.entry(tool).or_insert(0) += 1;
                    tally.1 += 1;
                }
                CellOutcome::Tie => tally.1 += 1,
                CellOutcome::Uncontested => {}
            }
        }

        let mut summaries: BTreeMap<CategoryKey, CategorySummary> = BTreeMap::new();
        for (category, (wins, contested_cells)) in tallies {
            let leader_wins: u64 = wins.values().copied().max().unwrap_or(0);
            let leaders: Vec<String> = wins
                .into_iter()
                .filter(|(_, count)| *count == leader_wins)
                .map(|(tool, _)| tool)
                .collect();
            summaries.insert(
                category,
                CategorySummary {
                    leaders,
                    leader_wins,
                    contested_cells,
                },
            );
        }
        summaries
    }

    /// Render the human-readable report written to `results.md`.
    ///
    /// Layout: methodology, tool versions, category winners, one table per (operation,
    /// metric) for each tier (fixtures as rows, tools as columns, outright winners **bold**,
    /// low-confidence values marked [`LOW_CONFIDENCE_MARK`]), then the compatibility matrix.
    #[must_use]
    pub fn to_markdown(&self) -> String {
        let outcomes: BTreeMap<CellKey, CellOutcome> = self.cell_outcomes();
        let mut out: String = String::new();
        out.push_str("# Benchmark results\n\n");
        out.push_str(&methodology());

        if !self.tool_versions.is_empty() {
            out.push_str("\n## Tool versions\n\n| Tool | Version |\n|---|---|\n");
            for (tool, version) in &self.tool_versions {
                let _ = writeln!(out, "| {} | {} |", escape_cell(tool), escape_cell(version));
            }
        }

        out.push_str("\n## Category winners\n\n");
        out.push_str(&self.render_category_winners());

        for tier in Tier::ALL {
            let _ = write!(out, "\n## {}\n", tier.heading());
            out.push_str(&self.render_tier(tier, &outcomes));
        }

        out.push_str("\n## Compatibility\n\n");
        out.push_str(&self.render_compat());
        out
    }

    /// The category winners table.
    fn render_category_winners(&self) -> String {
        let summaries: BTreeMap<CategoryKey, CategorySummary> = self.category_summaries();
        if summaries.is_empty() {
            return "No measurements.\n".to_owned();
        }
        let mut out: String = String::new();
        out.push_str("| Tier | Operation | Metric | Winner | Outright wins / contested |\n");
        out.push_str("|---|---|---|---|---|\n");
        for (category, summary) in &summaries {
            let (winner, wins): (String, String) = summary_cells(summary);
            let _ = writeln!(
                out,
                "| {} | {} | {} | {winner} | {wins} |",
                category.tier.label(),
                category.operation.label(),
                category.metric.label(category.tier),
            );
        }
        out
    }

    /// One tier's detailed tables (one per operation and metric with data).
    fn render_tier(&self, tier: Tier, outcomes: &BTreeMap<CellKey, CellOutcome>) -> String {
        let mut out: String = String::new();
        for operation in Operation::ALL {
            for metric in Metric::ALL {
                let mut cells: BTreeMap<(&str, &str), Stats> = BTreeMap::new();
                for measurement in &self.measurements {
                    if measurement.tier == tier
                        && measurement.operation == operation
                        && measurement.metric == metric
                    {
                        cells
                            .entry((measurement.fixture.as_str(), measurement.tool.as_str()))
                            .or_insert(measurement.stats);
                    }
                }
                if cells.is_empty() {
                    continue;
                }
                let fixtures: BTreeSet<&str> = cells.keys().map(|(fixture, _)| *fixture).collect();
                let tools: BTreeSet<&str> = cells.keys().map(|(_, tool)| *tool).collect();

                let _ = write!(
                    out,
                    "\n### {} — {}\n\n| Fixture |",
                    operation.label(),
                    metric.label(tier)
                );
                for tool in &tools {
                    let _ = write!(out, " {} |", escape_cell(tool));
                }
                out.push_str("\n|---|");
                for _ in &tools {
                    out.push_str("---|");
                }
                out.push('\n');

                for fixture in &fixtures {
                    let key: CellKey = CellKey {
                        tier,
                        operation,
                        metric,
                        fixture: (*fixture).to_owned(),
                    };
                    let winner: Option<&str> = match outcomes.get(&key) {
                        Some(CellOutcome::Outright(tool)) => Some(tool.as_str()),
                        Some(CellOutcome::Tie | CellOutcome::Uncontested) | None => None,
                    };
                    let _ = write!(out, "| {} |", escape_cell(fixture));
                    for tool in &tools {
                        match cells.get(&(*fixture, *tool)) {
                            Some(stats) => {
                                let cell: String =
                                    render_value(metric, stats, winner == Some(*tool));
                                let _ = write!(out, " {cell} |");
                            }
                            None => {
                                let _ = write!(out, " {MISSING_CELL} |");
                            }
                        }
                    }
                    out.push('\n');
                }
            }
        }
        if out.is_empty() {
            out.push_str("\nNo measurements.\n");
        }
        out
    }

    /// The compatibility matrix: rows are (tier, operation, fixture), columns are tools. Only
    /// rows with at least one failure are listed; a summary line counts the passing cells, so
    /// the report does not repeat dozens of all-`pass` rows.
    fn render_compat(&self) -> String {
        if self.compat.is_empty() {
            return "No compatibility entries.\n".to_owned();
        }
        let passed: usize = self
            .compat
            .iter()
            .filter(|entry: &&CompatEntry| entry.status == CompatStatus::Pass)
            .count();
        let attempted: usize = self.compat.len();
        if passed == attempted {
            return format!("All {attempted} attempted cells pass.\n");
        }
        let mut rows: BTreeMap<(Tier, Operation, &str), BTreeMap<&str, &CompatStatus>> =
            BTreeMap::new();
        let mut tools: BTreeSet<&str> = BTreeSet::new();
        for entry in &self.compat {
            tools.insert(entry.tool.as_str());
            rows.entry((entry.tier, entry.operation, entry.fixture.as_str()))
                .or_default()
                .entry(entry.tool.as_str())
                .or_insert(&entry.status);
        }

        rows.retain(|_key, statuses: &mut BTreeMap<&str, &CompatStatus>| {
            statuses
                .values()
                .any(|status: &&CompatStatus| **status != CompatStatus::Pass)
        });

        let mut out: String = format!(
            "{passed} of {attempted} attempted cells pass. Rows with at least one failure:\n\n"
        );
        out.push_str("| Tier | Operation | Fixture |");
        for tool in &tools {
            let _ = write!(out, " {} |", escape_cell(tool));
        }
        out.push_str("\n|---|---|---|");
        for _ in &tools {
            out.push_str("---|");
        }
        out.push('\n');
        for ((tier, operation, fixture), statuses) in &rows {
            let _ = write!(
                out,
                "| {} | {} | {} |",
                tier.label(),
                operation.label(),
                escape_cell(fixture)
            );
            for tool in &tools {
                let cell: String = match statuses.get(tool) {
                    Some(CompatStatus::Pass) => "pass".to_owned(),
                    Some(CompatStatus::Fail(reason)) if reason.is_empty() => "FAIL".to_owned(),
                    Some(CompatStatus::Fail(reason)) => format!("FAIL: {}", escape_cell(reason)),
                    None => MISSING_CELL.to_owned(),
                };
                let _ = write!(out, " {cell} |");
            }
            out.push('\n');
        }
        out
    }
}

/// Canonical sort key of a measurement (also its identity for duplicate detection).
fn measurement_order(measurement: &Measurement) -> (Tier, Operation, Metric, &str, &str) {
    (
        measurement.tier,
        measurement.operation,
        measurement.metric,
        measurement.fixture.as_str(),
        measurement.tool.as_str(),
    )
}

/// Canonical sort key of a compat entry (also its identity for duplicate detection).
fn compat_order(entry: &CompatEntry) -> (Tier, Operation, &str, &str) {
    (
        entry.tier,
        entry.operation,
        entry.fixture.as_str(),
        entry.tool.as_str(),
    )
}

/// The methodology paragraph at the top of the report.
fn methodology() -> String {
    format!(
        "Two tiers, lower is better everywhere. **In-process**: every tool is linked as a Rust \
         library into one process; time is criterion's median ± standard deviation per call, \
         and peak heap is the extra heap the operation allocates, recorded by a counting \
         allocator. **CLI**: every tool's binary runs end to end (process startup, file I/O, \
         and the operation), timed by hyperfine as median ± standard deviation over repeated \
         runs after warmup; peak RSS is the process's peak resident set size. A cell is \
         measured only when the tool is correct on it (see Compatibility). A cell's best tool \
         wins outright (**bold**) only when at least two tools were measured and its lead over \
         the runner-up exceeds both {:.0}% of its median and the sum of their standard \
         deviations; otherwise the cell is a tie or uncontested and nothing is bolded. \
         `{LOW_CONFIDENCE_MARK}` marks a measurement whose standard deviation exceeds {:.0}% of \
         its median.\n",
        TIE_THRESHOLD * 100.0,
        LOW_CONFIDENCE_RATIO * 100.0,
    )
}

/// The `Winner` and `Outright wins / contested` cells for one category.
fn summary_cells(summary: &CategorySummary) -> (String, String) {
    let contested: u64 = summary.contested_cells;
    if contested == 0 {
        return ("(no contested cells)".to_owned(), "0 / 0".to_owned());
    }
    match summary.leaders.as_slice() {
        [] => ("(all ties)".to_owned(), format!("0 / {contested}")),
        [leader] => (
            escape_cell(leader),
            format!("{} / {contested}", summary.leader_wins),
        ),
        leaders => (
            format!("{} (tied)", escape_cell(&leaders.join(", "))),
            format!("{} each / {contested}", summary.leader_wins),
        ),
    }
}

/// Render one table cell: `median ± stddev unit`, the low-confidence mark, and bold when the
/// tool won the cell outright.
#[must_use]
pub fn render_value(metric: Metric, stats: &Stats, outright: bool) -> String {
    let mut text: String = match metric {
        Metric::TimeSecs => format_time(stats.median, stats.stddev),
        Metric::PeakMemoryBytes => format_memory(stats.median, stats.stddev),
    };
    if stats.is_low_confidence() {
        text.push(' ');
        text.push_str(LOW_CONFIDENCE_MARK);
    }
    if outright {
        format!("**{text}**")
    } else {
        text
    }
}

/// Format a time as `median ± stddev unit`, choosing ns/µs/ms/s by the median's magnitude
/// (three decimals).
#[must_use]
pub fn format_time(median_secs: f64, stddev_secs: f64) -> String {
    let (scale, unit): (f64, &str) = if median_secs < 1e-6 {
        (1e9, "ns")
    } else if median_secs < 1e-3 {
        (1e6, "µs")
    } else if median_secs < 1.0 {
        (1e3, "ms")
    } else {
        (1.0, "s")
    };
    format!(
        "{:.3} ± {:.3} {unit}",
        median_secs * scale,
        stddev_secs * scale
    )
}

/// Format a byte count as KiB (below 1 MiB) or MiB, two decimals; `± stddev` only when the
/// spread is non-zero.
#[must_use]
pub fn format_memory(median_bytes: f64, stddev_bytes: f64) -> String {
    let (scale, unit): (f64, &str) = if median_bytes < MIB {
        (KIB, "KiB")
    } else {
        (MIB, "MiB")
    };
    if stddev_bytes > 0.0 {
        format!(
            "{:.2} ± {:.2} {unit}",
            median_bytes / scale,
            stddev_bytes / scale
        )
    } else {
        format!("{:.2} {unit}", median_bytes / scale)
    }
}

/// Escape a value for a Markdown table cell (`|` would split the cell).
fn escape_cell(text: &str) -> String {
    text.replace('|', "\\|")
}

/// The meaningful lines of a tab-separated capture: `(1-based line number, line)`, skipping
/// blank lines and `#` comments. A trailing `\r` is removed.
fn tsv_lines(text: &str) -> impl Iterator<Item = (usize, &str)> {
    text.lines().enumerate().filter_map(|(index, raw)| {
        let line: &str = raw.strip_suffix('\r').unwrap_or(raw);
        let trimmed: &str = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            None
        } else {
            Some((index + 1, line))
        }
    })
}

/// Split a TSV line into exactly `count` non-empty fields.
fn exact_fields(line: &str, count: usize) -> Result<Vec<&str>, String> {
    let fields: Vec<&str> = line.split('\t').collect();
    if fields.len() != count {
        return Err(format!(
            "expected {count} tab-separated fields, found {}",
            fields.len()
        ));
    }
    if let Some(position) = fields.iter().position(|field| field.trim().is_empty()) {
        return Err(format!("field {} is empty", position + 1));
    }
    Ok(fields)
}

/// Parse an operation token, with a skip reason on failure.
fn operation_field(token: &str) -> Result<Operation, String> {
    Operation::from_token(token).ok_or_else(|| format!("unknown operation `{token}`"))
}

/// Run `parse_row` on every meaningful line, collecting rows and skipped lines.
fn parse_tsv<T>(text: &str, parse_row: impl Fn(&str) -> Result<T, String>) -> TsvParse<T> {
    let mut parsed: TsvParse<T> = TsvParse::default();
    for (line_number, line) in tsv_lines(text) {
        match parse_row(line) {
            Ok(row) => parsed.rows.push(row),
            Err(reason) => parsed.skipped.push(SkippedLine {
                line: line_number,
                reason,
            }),
        }
    }
    parsed
}

/// Parse a compat capture (`inprocess_compat.tsv` or `cli/compat.tsv`) for `tier`.
///
/// Each row is `tool\tfixture\toperation\tstatus\tdetail`; `status` is `pass` or `fail` and
/// `detail` may be empty or absent.
#[must_use]
pub fn parse_compat_tsv(text: &str, tier: Tier) -> TsvParse<CompatEntry> {
    parse_tsv(text, |line: &str| {
        let fields: Vec<&str> = line.splitn(5, '\t').collect();
        let (tool, fixture, operation, status, detail): (&str, &str, &str, &str, &str) =
            match fields.as_slice() {
                [tool, fixture, operation, status] => (tool, fixture, operation, status, ""),
                [tool, fixture, operation, status, detail] => {
                    (tool, fixture, operation, status, detail)
                }
                _ => {
                    return Err(format!(
                        "expected 4 or 5 tab-separated fields, found {}",
                        fields.len()
                    ));
                }
            };
        if let Some(position) = [tool, fixture, operation, status]
            .iter()
            .position(|field| field.trim().is_empty())
        {
            return Err(format!("field {} is empty", position + 1));
        }
        let operation: Operation = operation_field(operation)?;
        let status: CompatStatus = match status {
            "pass" => CompatStatus::Pass,
            "fail" => CompatStatus::Fail(detail.trim().to_owned()),
            other => return Err(format!("unknown status `{other}`")),
        };
        Ok(CompatEntry {
            tier,
            tool: tool.to_owned(),
            fixture: fixture.to_owned(),
            operation,
            status,
        })
    })
}

/// Parse `inprocess_heap.tsv` (`tool\tfixture\toperation\tpeak_bytes`) into in-process
/// peak-memory measurements (one exact sample each).
#[must_use]
pub fn parse_heap_tsv(text: &str) -> TsvParse<Measurement> {
    parse_tsv(text, |line: &str| {
        let fields: Vec<&str> = exact_fields(line, 4)?;
        let operation: Operation = operation_field(fields[2])?;
        let bytes: u64 = fields[3]
            .trim()
            .parse::<u64>()
            .map_err(|_| format!("peak_bytes `{}` is not a non-negative integer", fields[3]))?;
        Ok(Measurement {
            tier: Tier::InProcess,
            tool: fields[0].to_owned(),
            fixture: fields[1].to_owned(),
            operation,
            metric: Metric::PeakMemoryBytes,
            stats: Stats::single(bytes_as_f64(bytes)),
        })
    })
}

/// Parse `cli/index.tsv` (`tool\tfixture\toperation\tjson_path`).
#[must_use]
pub fn parse_index_tsv(text: &str) -> TsvParse<IndexEntry> {
    parse_tsv(text, |line: &str| {
        let fields: Vec<&str> = exact_fields(line, 4)?;
        let operation: Operation = operation_field(fields[2])?;
        Ok(IndexEntry {
            tool: fields[0].to_owned(),
            fixture: fields[1].to_owned(),
            operation,
            json_path: fields[3].to_owned(),
        })
    })
}

/// Parse `tool_versions.tsv` (`tool\tversion`) into `(tool, version)` rows.
#[must_use]
pub fn parse_tool_versions_tsv(text: &str) -> TsvParse<(String, String)> {
    parse_tsv(text, |line: &str| {
        let fields: Vec<&str> = exact_fields(line, 2)?;
        Ok((fields[0].to_owned(), fields[1].trim().to_owned()))
    })
}

/// The subset of a hyperfine `--export-json` document the aggregator reads.
#[derive(Deserialize)]
struct HyperfineExport {
    results: Vec<HyperfineResult>,
}

/// One hyperfine command result.
#[derive(Deserialize)]
struct HyperfineResult {
    times: Vec<f64>,
    memory_usage_byte: Option<Vec<f64>>,
}

/// Parse a hyperfine export (one result) into CLI-tier measurements for a cell: a time
/// measurement from `results[0].times` (seconds) and, when present and non-empty, a peak-RSS
/// measurement from `results[0].memory_usage_byte`.
///
/// # Errors
///
/// Returns a reason when the JSON is invalid, has no result, or has no timing samples.
pub fn parse_hyperfine_json(
    json: &str,
    tool: &str,
    fixture: &str,
    operation: Operation,
) -> Result<Vec<Measurement>, String> {
    let export: HyperfineExport =
        serde_json::from_str(json).map_err(|e| format!("invalid hyperfine JSON: {e}"))?;
    let result: &HyperfineResult = export
        .results
        .first()
        .ok_or_else(|| "hyperfine export has no results".to_owned())?;
    let time: Stats = Stats::from_samples(&result.times)
        .ok_or_else(|| "hyperfine result has no timing samples".to_owned())?;

    let measurement = |metric: Metric, stats: Stats| -> Measurement {
        Measurement {
            tier: Tier::Cli,
            tool: tool.to_owned(),
            fixture: fixture.to_owned(),
            operation,
            metric,
            stats,
        }
    };
    let mut out: Vec<Measurement> = vec![measurement(Metric::TimeSecs, time)];
    if let Some(memory) = result
        .memory_usage_byte
        .as_deref()
        .and_then(Stats::from_samples)
    {
        out.push(measurement(Metric::PeakMemoryBytes, memory));
    }
    Ok(out)
}

/// The subset of criterion's `benchmark.json` the aggregator reads.
#[derive(Deserialize)]
struct CriterionBenchmark {
    group_id: String,
    function_id: Option<String>,
    value_str: Option<String>,
}

/// A criterion point estimate (nanoseconds).
#[derive(Deserialize)]
struct CriterionPointEstimate {
    point_estimate: f64,
}

/// The subset of criterion's `estimates.json` the aggregator reads.
#[derive(Deserialize)]
struct CriterionEstimates {
    median: CriterionPointEstimate,
    std_dev: CriterionPointEstimate,
}

/// The subset of criterion's `sample.json` the aggregator reads.
#[derive(Deserialize)]
struct CriterionSample {
    iters: Vec<f64>,
    times: Vec<f64>,
}

/// Parse a criterion `benchmark.json`: group id = operation, function id = tool,
/// `value_str` = fixture id.
///
/// Returns `Ok(None)` when the group is not a known operation (such a benchmark is not part
/// of the cross-tool harness).
///
/// # Errors
///
/// Returns a reason when the JSON is invalid or the function id or fixture id is missing.
pub fn parse_criterion_benchmark(json: &str) -> Result<Option<CriterionId>, String> {
    let benchmark: CriterionBenchmark =
        serde_json::from_str(json).map_err(|e| format!("invalid benchmark.json: {e}"))?;
    let Some(operation) = Operation::from_token(&benchmark.group_id) else {
        return Ok(None);
    };
    let tool: String = benchmark
        .function_id
        .filter(|tool| !tool.is_empty())
        .ok_or_else(|| "benchmark.json has no function_id (tool)".to_owned())?;
    let fixture: String = benchmark
        .value_str
        .filter(|fixture| !fixture.is_empty())
        .ok_or_else(|| "benchmark.json has no value_str (fixture id)".to_owned())?;
    Ok(Some(CriterionId {
        operation,
        tool,
        fixture,
    }))
}

/// Combine criterion's `estimates.json` (median and standard deviation estimates) and
/// `sample.json` (per-sample iteration counts and total times) into time statistics in
/// seconds. min/max are the per-iteration times `times[i] / iters[i]`; `samples` is the
/// sample count.
///
/// # Errors
///
/// Returns a reason when either document is invalid, the sample arrays are empty or differ
/// in length, or an iteration count is not positive.
pub fn parse_criterion_stats(estimates_json: &str, sample_json: &str) -> Result<Stats, String> {
    let estimates: CriterionEstimates =
        serde_json::from_str(estimates_json).map_err(|e| format!("invalid estimates.json: {e}"))?;
    let sample: CriterionSample =
        serde_json::from_str(sample_json).map_err(|e| format!("invalid sample.json: {e}"))?;
    if sample.iters.len() != sample.times.len() {
        return Err(format!(
            "sample.json has {} iters but {} times",
            sample.iters.len(),
            sample.times.len()
        ));
    }
    if sample.iters.iter().any(|iters| *iters <= 0.0) {
        return Err("sample.json has a non-positive iteration count".to_owned());
    }
    let per_iteration_secs: Vec<f64> = sample
        .times
        .iter()
        .zip(&sample.iters)
        .map(|(time, iters)| time / iters / 1e9)
        .collect();
    let spread: Stats = Stats::from_samples(&per_iteration_secs)
        .ok_or_else(|| "sample.json has no samples".to_owned())?;
    Ok(Stats {
        median: estimates.median.point_estimate / 1e9,
        stddev: estimates.std_dev.point_estimate / 1e9,
        min: spread.min,
        max: spread.max,
        samples: spread.samples,
    })
}

/// Whether a criterion output path is a `<...>/new/benchmark.json` file the aggregator
/// should read (criterion also keeps `base/` and `change/` copies, which are ignored).
#[must_use]
pub fn is_criterion_new_benchmark(path: &std::path::Path) -> bool {
    path.file_name().and_then(|name| name.to_str()) == Some("benchmark.json")
        && path
            .parent()
            .and_then(|parent| parent.file_name())
            .and_then(|name| name.to_str())
            == Some("new")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stats(median: f64, stddev: f64) -> Stats {
        Stats {
            median,
            stddev,
            min: median,
            max: median,
            samples: 10,
        }
    }

    fn measurement(
        tier: Tier,
        tool: &str,
        fixture: &str,
        operation: Operation,
        metric: Metric,
        stats: Stats,
    ) -> Measurement {
        Measurement {
            tier,
            tool: tool.to_owned(),
            fixture: fixture.to_owned(),
            operation,
            metric,
            stats,
        }
    }

    fn time(tool: &str, fixture: &str, median: f64, stddev: f64) -> Measurement {
        measurement(
            Tier::InProcess,
            tool,
            fixture,
            Operation::ValidateValid,
            Metric::TimeSecs,
            stats(median, stddev),
        )
    }

    fn compat(tier: Tier, tool: &str, fixture: &str, status: CompatStatus) -> CompatEntry {
        CompatEntry {
            tier,
            tool: tool.to_owned(),
            fixture: fixture.to_owned(),
            operation: Operation::ValidateValid,
            status,
        }
    }

    // ---- compatibility rendering ----------------------------------------------------------

    #[test]
    fn render_compat_summarizes_when_every_cell_passes() {
        let results: BenchmarkResults = BenchmarkResults {
            compat: vec![
                compat(Tier::InProcess, "boon", "small/a", CompatStatus::Pass),
                compat(Tier::Cli, "boon", "small/a", CompatStatus::Pass),
            ],
            ..BenchmarkResults::default()
        };
        let expected: String = "All 2 attempted cells pass.\n".to_owned();
        let actual: String = results.render_compat();
        assert_eq!(expected, actual);
    }

    #[test]
    fn render_compat_lists_only_rows_with_a_failure() {
        let results: BenchmarkResults = BenchmarkResults {
            compat: vec![
                compat(Tier::InProcess, "boon", "small/a", CompatStatus::Pass),
                compat(Tier::InProcess, "boon", "small/b", CompatStatus::Pass),
                compat(
                    Tier::InProcess,
                    "typify",
                    "small/b",
                    CompatStatus::Fail("panic".to_owned()),
                ),
            ],
            ..BenchmarkResults::default()
        };
        let expected: String = "2 of 3 attempted cells pass. Rows with at least one failure:\n\n\
            | Tier | Operation | Fixture | boon | typify |\n\
            |---|---|---|---|---|\n\
            | in-process | validate (valid) | small/b | pass | FAIL: panic |\n"
            .to_owned();
        let actual: String = results.render_compat();
        assert_eq!(expected, actual);
    }

    // ---- tokens -------------------------------------------------------------------------

    #[test]
    fn operation_serializes_as_snake_case_token() {
        let expected: String =
            "[\"codegen\",\"schema_compile\",\"validate_valid\",\"validate_invalid\"]".to_owned();
        let actual: String = serde_json::to_string(&Operation::ALL).expect("serializes");
        assert_eq!(expected, actual);
    }

    #[test]
    fn tier_and_metric_serialize_as_snake_case_tokens() {
        let expected: String =
            "[[\"in_process\",\"cli\"],[\"time_secs\",\"peak_memory_bytes\"]]".to_owned();
        let actual: String = serde_json::to_string(&(Tier::ALL, Metric::ALL)).expect("serializes");
        assert_eq!(expected, actual);
    }

    #[test]
    fn operation_from_token_round_trips_every_token() {
        let expected: Vec<Option<Operation>> = Operation::ALL.into_iter().map(Some).collect();
        let actual: Vec<Option<Operation>> = Operation::ALL
            .into_iter()
            .map(|operation: Operation| Operation::from_token(operation.token()))
            .collect();
        assert_eq!(expected, actual);
    }

    #[test]
    fn operation_from_token_rejects_unknown_token() {
        let expected: Option<Operation> = None;
        let actual: Option<Operation> = Operation::from_token("report");
        assert_eq!(expected, actual);
    }

    // ---- stats --------------------------------------------------------------------------

    #[test]
    fn stats_from_samples_odd_count() {
        let expected: Option<Stats> = Some(Stats {
            median: 2.0,
            stddev: 1.0,
            min: 1.0,
            max: 3.0,
            samples: 3,
        });
        let actual: Option<Stats> = Stats::from_samples(&[3.0, 1.0, 2.0]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn stats_from_samples_even_count_averages_middle_pair() {
        // mean 2.5; squared deviations 2.25 + 0.25 + 0.25 + 2.25 = 5; 5 / 3 -> sqrt.
        let expected: Option<Stats> = Some(Stats {
            median: 2.5,
            stddev: (5.0_f64 / 3.0).sqrt(),
            min: 1.0,
            max: 4.0,
            samples: 4,
        });
        let actual: Option<Stats> = Stats::from_samples(&[4.0, 1.0, 3.0, 2.0]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn stats_from_samples_single_sample_has_zero_stddev() {
        let expected: Option<Stats> = Some(Stats::single(7.0));
        let actual: Option<Stats> = Stats::from_samples(&[7.0]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn stats_from_samples_empty_is_none() {
        let expected: Option<Stats> = None;
        let actual: Option<Stats> = Stats::from_samples(&[]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn low_confidence_when_stddev_exceeds_ten_percent() {
        let expected: bool = true;
        let actual: bool = stats(1.0, 0.11).is_low_confidence();
        assert_eq!(expected, actual);
    }

    #[test]
    fn not_low_confidence_at_exactly_ten_percent() {
        let expected: bool = false;
        let actual: bool = stats(1.0, 0.1).is_low_confidence();
        assert_eq!(expected, actual);
    }

    #[test]
    fn zero_median_with_spread_is_low_confidence() {
        let expected: bool = true;
        let actual: bool = stats(0.0, 1.0).is_low_confidence();
        assert_eq!(expected, actual);
    }

    #[test]
    fn zero_median_without_spread_is_not_low_confidence() {
        let expected: bool = false;
        let actual: bool = stats(0.0, 0.0).is_low_confidence();
        assert_eq!(expected, actual);
    }

    // ---- winner rule --------------------------------------------------------------------

    #[test]
    fn decide_cell_outright_when_gap_beats_threshold_and_noise() {
        let expected: Option<CellOutcome> = Some(CellOutcome::Outright("ours".to_owned()));
        let actual: Option<CellOutcome> =
            decide_cell(&[("boon", stats(2.0, 0.1)), ("ours", stats(1.0, 0.1))]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn decide_cell_tie_when_gap_within_threshold() {
        // Gap 0.02 < 3% of 1.0 even though the spread is zero.
        let expected: Option<CellOutcome> = Some(CellOutcome::Tie);
        let actual: Option<CellOutcome> =
            decide_cell(&[("ours", stats(1.0, 0.0)), ("boon", stats(1.02, 0.0))]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn decide_cell_tie_when_gap_within_noise() {
        // Gap 0.5 beats 3% of 1.0 but not the summed stddev 0.3 + 0.3.
        let expected: Option<CellOutcome> = Some(CellOutcome::Tie);
        let actual: Option<CellOutcome> =
            decide_cell(&[("ours", stats(1.0, 0.3)), ("boon", stats(1.5, 0.3))]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn decide_cell_compares_best_against_runner_up_only() {
        // Best vs runner-up is a tie even though a third tool is far behind.
        let expected: Option<CellOutcome> = Some(CellOutcome::Tie);
        let actual: Option<CellOutcome> = decide_cell(&[
            ("third", stats(9.0, 0.0)),
            ("ours", stats(1.0, 0.0)),
            ("boon", stats(1.01, 0.0)),
        ]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn decide_cell_uncontested_with_one_tool() {
        let expected: Option<CellOutcome> = Some(CellOutcome::Uncontested);
        let actual: Option<CellOutcome> = decide_cell(&[("ours", stats(1.0, 0.0))]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn decide_cell_empty_is_none() {
        let expected: Option<CellOutcome> = None;
        let actual: Option<CellOutcome> = decide_cell(&[]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn cell_outcomes_keys_every_cell() {
        let results: BenchmarkResults = BenchmarkResults {
            measurements: vec![
                time("ours", "a", 1.0, 0.0),
                time("boon", "a", 2.0, 0.0),
                time("ours", "b", 1.0, 0.0),
            ],
            ..Default::default()
        };
        let key = |fixture: &str| -> CellKey {
            CellKey {
                tier: Tier::InProcess,
                operation: Operation::ValidateValid,
                metric: Metric::TimeSecs,
                fixture: fixture.to_owned(),
            }
        };
        let expected: BTreeMap<CellKey, CellOutcome> = BTreeMap::from([
            (key("a"), CellOutcome::Outright("ours".to_owned())),
            (key("b"), CellOutcome::Uncontested),
        ]);
        let actual: BTreeMap<CellKey, CellOutcome> = results.cell_outcomes();
        assert_eq!(expected, actual);
    }

    // ---- champion tally -----------------------------------------------------------------

    fn category() -> CategoryKey {
        CategoryKey {
            tier: Tier::InProcess,
            operation: Operation::ValidateValid,
            metric: Metric::TimeSecs,
        }
    }

    #[test]
    fn category_summary_counts_outright_wins_and_contested_cells() {
        let results: BenchmarkResults = BenchmarkResults {
            measurements: vec![
                // a: ours outright. b: ours outright. c: tie. d: uncontested.
                time("ours", "a", 1.0, 0.0),
                time("boon", "a", 2.0, 0.0),
                time("ours", "b", 1.0, 0.0),
                time("boon", "b", 3.0, 0.0),
                time("ours", "c", 1.0, 0.0),
                time("boon", "c", 1.01, 0.0),
                time("ours", "d", 1.0, 0.0),
            ],
            ..Default::default()
        };
        let expected: BTreeMap<CategoryKey, CategorySummary> = BTreeMap::from([(
            category(),
            CategorySummary {
                leaders: vec!["ours".to_owned()],
                leader_wins: 2,
                contested_cells: 3,
            },
        )]);
        let actual: BTreeMap<CategoryKey, CategorySummary> = results.category_summaries();
        assert_eq!(expected, actual);
    }

    #[test]
    fn category_summary_lists_every_tool_sharing_the_top_count() {
        let results: BenchmarkResults = BenchmarkResults {
            measurements: vec![
                time("ours", "a", 1.0, 0.0),
                time("boon", "a", 2.0, 0.0),
                time("ours", "b", 2.0, 0.0),
                time("boon", "b", 1.0, 0.0),
            ],
            ..Default::default()
        };
        let expected: BTreeMap<CategoryKey, CategorySummary> = BTreeMap::from([(
            category(),
            CategorySummary {
                leaders: vec!["boon".to_owned(), "ours".to_owned()],
                leader_wins: 1,
                contested_cells: 2,
            },
        )]);
        let actual: BTreeMap<CategoryKey, CategorySummary> = results.category_summaries();
        assert_eq!(expected, actual);
    }

    #[test]
    fn category_summary_without_contested_cells_has_no_leaders() {
        let results: BenchmarkResults = BenchmarkResults {
            measurements: vec![time("ours", "a", 1.0, 0.0)],
            ..Default::default()
        };
        let expected: BTreeMap<CategoryKey, CategorySummary> = BTreeMap::from([(
            category(),
            CategorySummary {
                leaders: vec![],
                leader_wins: 0,
                contested_cells: 0,
            },
        )]);
        let actual: BTreeMap<CategoryKey, CategorySummary> = results.category_summaries();
        assert_eq!(expected, actual);
    }

    #[test]
    fn summary_cells_single_leader() {
        let summary: CategorySummary = CategorySummary {
            leaders: vec!["ours".to_owned()],
            leader_wins: 2,
            contested_cells: 3,
        };
        let expected: (String, String) = ("ours".to_owned(), "2 / 3".to_owned());
        let actual: (String, String) = summary_cells(&summary);
        assert_eq!(expected, actual);
    }

    #[test]
    fn summary_cells_shared_leaders() {
        let summary: CategorySummary = CategorySummary {
            leaders: vec!["boon".to_owned(), "ours".to_owned()],
            leader_wins: 1,
            contested_cells: 2,
        };
        let expected: (String, String) = ("boon, ours (tied)".to_owned(), "1 each / 2".to_owned());
        let actual: (String, String) = summary_cells(&summary);
        assert_eq!(expected, actual);
    }

    #[test]
    fn summary_cells_all_ties() {
        let summary: CategorySummary = CategorySummary {
            leaders: vec![],
            leader_wins: 0,
            contested_cells: 2,
        };
        let expected: (String, String) = ("(all ties)".to_owned(), "0 / 2".to_owned());
        let actual: (String, String) = summary_cells(&summary);
        assert_eq!(expected, actual);
    }

    #[test]
    fn summary_cells_no_contested_cells() {
        let summary: CategorySummary = CategorySummary {
            leaders: vec![],
            leader_wins: 0,
            contested_cells: 0,
        };
        let expected: (String, String) = ("(no contested cells)".to_owned(), "0 / 0".to_owned());
        let actual: (String, String) = summary_cells(&summary);
        assert_eq!(expected, actual);
    }

    // ---- normalize ----------------------------------------------------------------------

    #[test]
    fn normalize_sorts_and_drops_duplicate_measurements() {
        let mut results: BenchmarkResults = BenchmarkResults {
            measurements: vec![
                time("ours", "b", 1.0, 0.0),
                time("ours", "a", 1.0, 0.0),
                time("ours", "b", 5.0, 0.0),
            ],
            ..Default::default()
        };
        let _ = results.normalize();
        let expected: Vec<Measurement> =
            vec![time("ours", "a", 1.0, 0.0), time("ours", "b", 1.0, 0.0)];
        let actual: Vec<Measurement> = results.measurements;
        assert_eq!(expected, actual);
    }

    #[test]
    fn normalize_sorts_and_drops_duplicate_compat_entries() {
        let mut results: BenchmarkResults = BenchmarkResults {
            compat: vec![
                compat(Tier::Cli, "ours", "a", CompatStatus::Pass),
                compat(Tier::InProcess, "ours", "a", CompatStatus::Pass),
                compat(Tier::Cli, "ours", "a", CompatStatus::Fail("x".to_owned())),
            ],
            ..Default::default()
        };
        let _ = results.normalize();
        let expected: Vec<CompatEntry> = vec![
            compat(Tier::InProcess, "ours", "a", CompatStatus::Pass),
            compat(Tier::Cli, "ours", "a", CompatStatus::Pass),
        ];
        let actual: Vec<CompatEntry> = results.compat;
        assert_eq!(expected, actual);
    }

    #[test]
    fn normalize_reports_each_dropped_duplicate() {
        let mut results: BenchmarkResults = BenchmarkResults {
            measurements: vec![time("ours", "a", 1.0, 0.0), time("ours", "a", 2.0, 0.0)],
            compat: vec![
                compat(Tier::Cli, "ours", "a", CompatStatus::Pass),
                compat(Tier::Cli, "ours", "a", CompatStatus::Pass),
            ],
            ..Default::default()
        };
        let expected: Vec<String> = vec![
            "duplicate measurement dropped: in-process ours a validate_valid time".to_owned(),
            "duplicate compat entry dropped: CLI ours a validate_valid".to_owned(),
        ];
        let actual: Vec<String> = results.normalize();
        assert_eq!(expected, actual);
    }

    // ---- formatting ---------------------------------------------------------------------

    #[test]
    fn format_time_just_below_one_microsecond_uses_ns() {
        let expected: String = "999.000 ± 1.000 ns".to_owned();
        let actual: String = format_time(999e-9, 1e-9);
        assert_eq!(expected, actual);
    }

    #[test]
    fn format_time_at_one_microsecond_uses_us() {
        let expected: String = "1.000 ± 0.010 µs".to_owned();
        let actual: String = format_time(1e-6, 1e-8);
        assert_eq!(expected, actual);
    }

    #[test]
    fn format_time_at_one_millisecond_uses_ms() {
        let expected: String = "1.000 ± 0.250 ms".to_owned();
        let actual: String = format_time(1e-3, 2.5e-4);
        assert_eq!(expected, actual);
    }

    #[test]
    fn format_time_at_one_second_uses_s() {
        let expected: String = "1.500 ± 0.000 s".to_owned();
        let actual: String = format_time(1.5, 0.0);
        assert_eq!(expected, actual);
    }

    #[test]
    fn format_memory_below_one_mib_uses_kib_without_spread() {
        let expected: String = "1023.50 KiB".to_owned();
        let actual: String = format_memory(1_048_064.0, 0.0);
        assert_eq!(expected, actual);
    }

    #[test]
    fn format_memory_at_one_mib_uses_mib_with_spread() {
        let expected: String = "1.00 ± 0.50 MiB".to_owned();
        let actual: String = format_memory(1_048_576.0, 524_288.0);
        assert_eq!(expected, actual);
    }

    #[test]
    fn render_value_bolds_outright_and_marks_low_confidence() {
        let expected: String = "**2.000 ± 0.500 ms †**".to_owned();
        let actual: String = render_value(Metric::TimeSecs, &stats(2e-3, 5e-4), true);
        assert_eq!(expected, actual);
    }

    #[test]
    fn render_value_plain_when_not_outright_and_confident() {
        let expected: String = "2.00 KiB".to_owned();
        let actual: String = render_value(Metric::PeakMemoryBytes, &Stats::single(2048.0), false);
        assert_eq!(expected, actual);
    }

    // ---- TSV parsers --------------------------------------------------------------------

    #[test]
    fn parse_compat_tsv_reads_pass_fail_and_skips_malformed() {
        let text: &str = "# comment\n\
            ours\tsmall/config\tcodegen\tpass\t\n\
            \n\
            typify\tsmall/config\tcodegen\tfail\tpanicked: oneOf\r\n\
            boon\tsmall/config\tvalidate_valid\tfail\n\
            boon\tsmall/config\tbogus_op\tpass\t\n\
            boon\tsmall/config\tcodegen\tmaybe\t\n\
            short\tline\n\
            \tsmall/config\tcodegen\tpass\t\n";
        let expected: TsvParse<CompatEntry> = TsvParse {
            rows: vec![
                CompatEntry {
                    tier: Tier::Cli,
                    tool: "ours".to_owned(),
                    fixture: "small/config".to_owned(),
                    operation: Operation::Codegen,
                    status: CompatStatus::Pass,
                },
                CompatEntry {
                    tier: Tier::Cli,
                    tool: "typify".to_owned(),
                    fixture: "small/config".to_owned(),
                    operation: Operation::Codegen,
                    status: CompatStatus::Fail("panicked: oneOf".to_owned()),
                },
                CompatEntry {
                    tier: Tier::Cli,
                    tool: "boon".to_owned(),
                    fixture: "small/config".to_owned(),
                    operation: Operation::ValidateValid,
                    status: CompatStatus::Fail(String::new()),
                },
            ],
            skipped: vec![
                SkippedLine {
                    line: 6,
                    reason: "unknown operation `bogus_op`".to_owned(),
                },
                SkippedLine {
                    line: 7,
                    reason: "unknown status `maybe`".to_owned(),
                },
                SkippedLine {
                    line: 8,
                    reason: "expected 4 or 5 tab-separated fields, found 2".to_owned(),
                },
                SkippedLine {
                    line: 9,
                    reason: "field 1 is empty".to_owned(),
                },
            ],
        };
        let actual: TsvParse<CompatEntry> = parse_compat_tsv(text, Tier::Cli);
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_heap_tsv_reads_bytes_and_skips_malformed() {
        let text: &str = "ours\tsmall/config\tschema_compile\t2048\n\
            ours\tsmall/config\tschema_compile\t-1\n\
            ours\tsmall/config\tschema_compile\n";
        let expected: TsvParse<Measurement> = TsvParse {
            rows: vec![measurement(
                Tier::InProcess,
                "ours",
                "small/config",
                Operation::SchemaCompile,
                Metric::PeakMemoryBytes,
                Stats::single(2048.0),
            )],
            skipped: vec![
                SkippedLine {
                    line: 2,
                    reason: "peak_bytes `-1` is not a non-negative integer".to_owned(),
                },
                SkippedLine {
                    line: 3,
                    reason: "expected 4 tab-separated fields, found 3".to_owned(),
                },
            ],
        };
        let actual: TsvParse<Measurement> = parse_heap_tsv(text);
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_index_tsv_reads_entries_and_skips_malformed() {
        let text: &str = "ours\tsmall/config\tvalidate_invalid\tcli/runs/ours.json\n\
            ours\tsmall/config\tnope\tcli/runs/x.json\n";
        let expected: TsvParse<IndexEntry> = TsvParse {
            rows: vec![IndexEntry {
                tool: "ours".to_owned(),
                fixture: "small/config".to_owned(),
                operation: Operation::ValidateInvalid,
                json_path: "cli/runs/ours.json".to_owned(),
            }],
            skipped: vec![SkippedLine {
                line: 2,
                reason: "unknown operation `nope`".to_owned(),
            }],
        };
        let actual: TsvParse<IndexEntry> = parse_index_tsv(text);
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_tool_versions_tsv_reads_pairs_and_skips_malformed() {
        let text: &str = "typify\tcargo-typify 0.7.0\nboon\n";
        let expected: TsvParse<(String, String)> = TsvParse {
            rows: vec![("typify".to_owned(), "cargo-typify 0.7.0".to_owned())],
            skipped: vec![SkippedLine {
                line: 2,
                reason: "expected 2 tab-separated fields, found 1".to_owned(),
            }],
        };
        let actual: TsvParse<(String, String)> = parse_tool_versions_tsv(text);
        assert_eq!(expected, actual);
    }

    // ---- hyperfine ----------------------------------------------------------------------

    #[test]
    fn parse_hyperfine_json_yields_time_and_memory() {
        let json: &str = r#"{"results":[{"command":"x","mean":0.0,"times":[0.003,0.001,0.002],
            "memory_usage_byte":[2048,1024,3072],"exit_codes":[0,0,0]}]}"#;
        let expected: Result<Vec<Measurement>, String> = Ok(vec![
            measurement(
                Tier::Cli,
                "ours",
                "small/config",
                Operation::Codegen,
                Metric::TimeSecs,
                Stats::from_samples(&[0.003, 0.001, 0.002]).expect("samples"),
            ),
            measurement(
                Tier::Cli,
                "ours",
                "small/config",
                Operation::Codegen,
                Metric::PeakMemoryBytes,
                Stats {
                    median: 2048.0,
                    stddev: 1024.0,
                    min: 1024.0,
                    max: 3072.0,
                    samples: 3,
                },
            ),
        ]);
        let actual: Result<Vec<Measurement>, String> =
            parse_hyperfine_json(json, "ours", "small/config", Operation::Codegen);
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_hyperfine_json_without_memory_yields_time_only() {
        let json: &str = r#"{"results":[{"times":[0.5]}]}"#;
        let expected: Result<Vec<Measurement>, String> = Ok(vec![measurement(
            Tier::Cli,
            "ours",
            "f",
            Operation::Codegen,
            Metric::TimeSecs,
            Stats::single(0.5),
        )]);
        let actual: Result<Vec<Measurement>, String> =
            parse_hyperfine_json(json, "ours", "f", Operation::Codegen);
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_hyperfine_json_rejects_empty_results() {
        let expected: Result<Vec<Measurement>, String> =
            Err("hyperfine export has no results".to_owned());
        let actual: Result<Vec<Measurement>, String> =
            parse_hyperfine_json(r#"{"results":[]}"#, "ours", "f", Operation::Codegen);
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_hyperfine_json_rejects_empty_times() {
        let expected: Result<Vec<Measurement>, String> =
            Err("hyperfine result has no timing samples".to_owned());
        let actual: Result<Vec<Measurement>, String> = parse_hyperfine_json(
            r#"{"results":[{"times":[]}]}"#,
            "ours",
            "f",
            Operation::Codegen,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_hyperfine_json_rejects_invalid_json() {
        let expected: Result<Vec<Measurement>, String> = Err(
            "invalid hyperfine JSON: EOF while parsing an object at line 1 column 1".to_owned(),
        );
        let actual: Result<Vec<Measurement>, String> =
            parse_hyperfine_json("{", "ours", "f", Operation::Codegen);
        assert_eq!(expected, actual);
    }

    // ---- criterion ----------------------------------------------------------------------

    #[test]
    fn parse_criterion_benchmark_reads_operation_tool_and_fixture() {
        let json: &str = r#"{"group_id":"schema_compile","function_id":"boon",
            "value_str":"small/config","throughput":null}"#;
        let expected: Result<Option<CriterionId>, String> = Ok(Some(CriterionId {
            operation: Operation::SchemaCompile,
            tool: "boon".to_owned(),
            fixture: "small/config".to_owned(),
        }));
        let actual: Result<Option<CriterionId>, String> = parse_criterion_benchmark(json);
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_criterion_benchmark_ignores_unknown_group() {
        let json: &str = r#"{"group_id":"report","function_id":"x","value_str":"y"}"#;
        let expected: Result<Option<CriterionId>, String> = Ok(None);
        let actual: Result<Option<CriterionId>, String> = parse_criterion_benchmark(json);
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_criterion_benchmark_rejects_missing_fixture() {
        let json: &str = r#"{"group_id":"codegen","function_id":"ours","value_str":null}"#;
        let expected: Result<Option<CriterionId>, String> =
            Err("benchmark.json has no value_str (fixture id)".to_owned());
        let actual: Result<Option<CriterionId>, String> = parse_criterion_benchmark(json);
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_criterion_benchmark_rejects_missing_tool() {
        let json: &str = r#"{"group_id":"codegen","function_id":null,"value_str":"f"}"#;
        let expected: Result<Option<CriterionId>, String> =
            Err("benchmark.json has no function_id (tool)".to_owned());
        let actual: Result<Option<CriterionId>, String> = parse_criterion_benchmark(json);
        assert_eq!(expected, actual);
    }

    const ESTIMATES: &str = r#"{"mean":{"point_estimate":2500.0},
        "median":{"point_estimate":2000.0},"std_dev":{"point_estimate":100.0}}"#;

    #[test]
    fn parse_criterion_stats_combines_estimates_and_samples() {
        // Per-iteration times: 3000/1 = 3000 ns, 4000/2 = 2000 ns, 4500/3 = 1500 ns.
        let sample: &str = r#"{"sampling_mode":"Linear","iters":[1.0,2.0,3.0],
            "times":[3000.0,4000.0,4500.0]}"#;
        let expected: Result<Stats, String> = Ok(Stats {
            median: 2e-6,
            stddev: 1e-7,
            min: 1.5e-6,
            max: 3e-6,
            samples: 3,
        });
        let actual: Result<Stats, String> = parse_criterion_stats(ESTIMATES, sample);
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_criterion_stats_rejects_mismatched_lengths() {
        let sample: &str = r#"{"iters":[1.0,2.0],"times":[3000.0]}"#;
        let expected: Result<Stats, String> = Err("sample.json has 2 iters but 1 times".to_owned());
        let actual: Result<Stats, String> = parse_criterion_stats(ESTIMATES, sample);
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_criterion_stats_rejects_zero_iterations() {
        let sample: &str = r#"{"iters":[0.0],"times":[3000.0]}"#;
        let expected: Result<Stats, String> =
            Err("sample.json has a non-positive iteration count".to_owned());
        let actual: Result<Stats, String> = parse_criterion_stats(ESTIMATES, sample);
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_criterion_stats_rejects_empty_samples() {
        let sample: &str = r#"{"iters":[],"times":[]}"#;
        let expected: Result<Stats, String> = Err("sample.json has no samples".to_owned());
        let actual: Result<Stats, String> = parse_criterion_stats(ESTIMATES, sample);
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_criterion_stats_rejects_estimates_without_std_dev() {
        let sample: &str = r#"{"iters":[1.0],"times":[3000.0]}"#;
        let expected: Result<Stats, String> =
            Err("invalid estimates.json: missing field `std_dev` at line 1 column 33".to_owned());
        let actual: Result<Stats, String> =
            parse_criterion_stats(r#"{"median":{"point_estimate":1.0}}"#, sample);
        assert_eq!(expected, actual);
    }

    #[test]
    fn is_criterion_new_benchmark_accepts_new_dir() {
        let expected: bool = true;
        let actual: bool = is_criterion_new_benchmark(std::path::Path::new(
            "target/criterion/codegen/ours/small_config/new/benchmark.json",
        ));
        assert_eq!(expected, actual);
    }

    #[test]
    fn is_criterion_new_benchmark_rejects_base_dir() {
        let expected: bool = false;
        let actual: bool = is_criterion_new_benchmark(std::path::Path::new(
            "target/criterion/codegen/ours/small_config/base/benchmark.json",
        ));
        assert_eq!(expected, actual);
    }

    // ---- markdown -----------------------------------------------------------------------

    #[test]
    fn markdown_renders_empty_results() {
        let results: BenchmarkResults = BenchmarkResults::default();
        let expected: String = format!(
            "# Benchmark results\n\n{}\n## Category winners\n\nNo measurements.\n\n\
             ## In-process (library calls)\n\nNo measurements.\n\n\
             ## CLI (end to end)\n\nNo measurements.\n\n\
             ## Compatibility\n\nNo compatibility entries.\n",
            methodology()
        );
        let actual: String = results.to_markdown();
        assert_eq!(expected, actual);
    }

    #[test]
    fn markdown_golden_small_run() {
        let results: BenchmarkResults = BenchmarkResults {
            tool_versions: BTreeMap::from([
                ("boon".to_owned(), "0.6.1".to_owned()),
                ("jsonschemars".to_owned(), "0.0.6".to_owned()),
            ]),
            measurements: vec![
                // In-process validate (valid) time: a = outright, b = tie by noise.
                time("jsonschemars", "small/a", 1e-6, 1e-8),
                time("boon", "small/a", 2e-6, 1e-8),
                time("jsonschemars", "small/b", 1e-6, 3e-7),
                time("boon", "small/b", 1.2e-6, 1e-8),
                // In-process peak heap: uncontested.
                measurement(
                    Tier::InProcess,
                    "jsonschemars",
                    "small/a",
                    Operation::ValidateValid,
                    Metric::PeakMemoryBytes,
                    Stats::single(4096.0),
                ),
                // CLI codegen time + RSS: outright time, tied RSS.
                measurement(
                    Tier::Cli,
                    "jsonschemars",
                    "small/a",
                    Operation::Codegen,
                    Metric::TimeSecs,
                    stats(2e-3, 1e-4),
                ),
                measurement(
                    Tier::Cli,
                    "typify",
                    "small/a",
                    Operation::Codegen,
                    Metric::TimeSecs,
                    stats(9e-3, 2e-4),
                ),
                measurement(
                    Tier::Cli,
                    "jsonschemars",
                    "small/a",
                    Operation::Codegen,
                    Metric::PeakMemoryBytes,
                    stats(3_145_728.0, 0.0),
                ),
                measurement(
                    Tier::Cli,
                    "typify",
                    "small/a",
                    Operation::Codegen,
                    Metric::PeakMemoryBytes,
                    stats(3_150_000.0, 0.0),
                ),
            ],
            compat: vec![
                compat(
                    Tier::InProcess,
                    "jsonschemars",
                    "small/a",
                    CompatStatus::Pass,
                ),
                compat(Tier::InProcess, "boon", "small/a", CompatStatus::Pass),
                compat(
                    Tier::Cli,
                    "typify",
                    "small/b",
                    CompatStatus::Fail("panicked | oneOf".to_owned()),
                ),
            ],
        };
        let expected: String = format!(
            "# Benchmark results\n\n{}{}",
            methodology(),
            GOLDEN_AFTER_METHODOLOGY
        );
        let actual: String = results.to_markdown();
        assert_eq!(expected, actual);
    }

    const GOLDEN_AFTER_METHODOLOGY: &str = r"
## Tool versions

| Tool | Version |
|---|---|
| boon | 0.6.1 |
| jsonschemars | 0.0.6 |

## Category winners

| Tier | Operation | Metric | Winner | Outright wins / contested |
|---|---|---|---|---|
| in-process | validate (valid) | time | jsonschemars | 1 / 2 |
| in-process | validate (valid) | peak heap | (no contested cells) | 0 / 0 |
| CLI | codegen | time | jsonschemars | 1 / 1 |
| CLI | codegen | peak RSS | (all ties) | 0 / 1 |

## In-process (library calls)

### validate (valid) — time

| Fixture | boon | jsonschemars |
|---|---|---|
| small/a | 2.000 ± 0.010 µs | **1.000 ± 0.010 µs** |
| small/b | 1.200 ± 0.010 µs | 1.000 ± 0.300 µs † |

### validate (valid) — peak heap

| Fixture | jsonschemars |
|---|---|
| small/a | 4.00 KiB |

## CLI (end to end)

### codegen — time

| Fixture | jsonschemars | typify |
|---|---|---|
| small/a | **2.000 ± 0.100 ms** | 9.000 ± 0.200 ms |

### codegen — peak RSS

| Fixture | jsonschemars | typify |
|---|---|---|
| small/a | 3.00 MiB | 3.00 MiB |

## Compatibility

2 of 3 attempted cells pass. Rows with at least one failure:

| Tier | Operation | Fixture | boon | jsonschemars | typify |
|---|---|---|---|---|---|
| CLI | validate (valid) | small/b | — | — | FAIL: panicked \| oneOf |
";
}
