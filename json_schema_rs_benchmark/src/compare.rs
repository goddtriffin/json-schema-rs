//! The quick benchmark's comparison (`make benchmark`): our tool's cells measured on a base
//! commit and on the working tree, in the same session, and which of them moved beyond noise.
//!
//! The driver (`scripts/quick_benchmark.sh`) builds both sides, measures them in interleaved
//! rounds, and collects every raw sample per cell and side into [`Samples`]; the `compare`
//! binary reads those captures, calls [`compare`], and renders [`Comparison::to_markdown`],
//! the PR body's *Performance* section. Pooling every round's samples before summarizing means
//! machine drift during the run lands on both sides alike.
//!
//! A cell moved when its medians differ by more than [`TIE_THRESHOLD`] of the base median
//! **and** by more than the sum of both standard deviations: the rule the full benchmark uses
//! to name a category winner ([`crate::decide_cell`]).

use crate::{CellKey, Metric, Stats, TIE_THRESHOLD, format_memory, format_time};
use std::collections::BTreeMap;
use std::fmt::Write as _;

/// Every raw sample of one side, by cell: seconds per call or run for time, bytes for memory.
pub type Samples = BTreeMap<CellKey, Vec<f64>>;

/// Append `values` to the samples of `key`.
pub fn add_samples(samples: &mut Samples, key: CellKey, values: impl IntoIterator<Item = f64>) {
    samples.entry(key).or_default().extend(values);
}

/// One cell measured on both sides.
#[derive(Debug, Clone, PartialEq)]
pub struct CellChange {
    /// The cell.
    pub key: CellKey,
    /// Statistics of the base side's pooled samples.
    pub base: Stats,
    /// Statistics of the head side's pooled samples.
    pub head: Stats,
}

impl CellChange {
    /// The head median relative to the base median (`-0.1` is 10% lower, so better). Zero when
    /// the base median is zero.
    #[must_use]
    pub fn relative(&self) -> f64 {
        if self.base.median == 0.0 {
            return 0.0;
        }
        (self.head.median - self.base.median) / self.base.median
    }
}

/// The outcome of comparing both sides.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Comparison {
    /// Cells that moved beyond noise, best change first (then by cell).
    pub moved: Vec<CellChange>,
    /// How many cells were measured on both sides and did not move.
    pub steady: usize,
    /// Cells measured only on the base side (the head side no longer measures them, usually
    /// because the cell stopped passing its correctness check).
    pub base_only: Vec<CellKey>,
    /// Cells measured only on the head side.
    pub head_only: Vec<CellKey>,
}

/// Whether `head` differs from `base` beyond noise: the medians differ by more than
/// [`TIE_THRESHOLD`] of the base median and by more than the sum of both standard deviations.
#[must_use]
pub fn moved_beyond_noise(base: &Stats, head: &Stats) -> bool {
    let gap: f64 = (head.median - base.median).abs();
    gap > (TIE_THRESHOLD * base.median).max(base.stddev + head.stddev)
}

/// Compare every cell of both sides. A cell with no samples on a side counts as not measured
/// there.
#[must_use]
pub fn compare(base: &Samples, head: &Samples) -> Comparison {
    let mut comparison: Comparison = Comparison::default();
    for (key, base_samples) in base {
        let Some(base_stats) = Stats::from_samples(base_samples) else {
            continue;
        };
        let Some(head_stats) = head.get(key).and_then(|values| Stats::from_samples(values)) else {
            comparison.base_only.push(key.clone());
            continue;
        };
        if moved_beyond_noise(&base_stats, &head_stats) {
            comparison.moved.push(CellChange {
                key: key.clone(),
                base: base_stats,
                head: head_stats,
            });
        } else {
            comparison.steady += 1;
        }
    }
    for (key, head_samples) in head {
        let measured_on_base: bool = base.get(key).is_some_and(|values| !values.is_empty());
        if !head_samples.is_empty() && !measured_on_base {
            comparison.head_only.push(key.clone());
        }
    }
    comparison.moved.sort_by(|a: &CellChange, b: &CellChange| {
        a.relative()
            .total_cmp(&b.relative())
            .then_with(|| a.key.cmp(&b.key))
    });
    comparison
}

impl Comparison {
    /// The PR body's *Performance* section: a one-line summary, then one collapsed table per
    /// metric (memory, then time) listing every moved cell, best change first, and the cells
    /// measured on one side only. `base_label` names the base side (usually `main`).
    #[must_use]
    pub fn to_markdown(&self, base_label: &str) -> String {
        let compared: usize = self.moved.len() + self.steady;
        let mut out: String = String::new();
        if self.moved.is_empty() {
            let _ = writeln!(
                out,
                "Nothing moved beyond noise ({compared} cells of our library compared against \
                 `{base_label}`, measured in the same session)."
            );
        } else {
            let better: usize = self
                .moved
                .iter()
                .filter(|change: &&CellChange| change.relative() < 0.0)
                .count();
            let _ = writeln!(
                out,
                "{} of {compared} cells of our library moved beyond noise against \
                 `{base_label}`, measured in the same session: {better} better, {} worse.",
                self.moved.len(),
                self.moved.len() - better
            );
            for (metric, title) in [
                (Metric::PeakMemoryBytes, "Memory"),
                (Metric::TimeSecs, "Time"),
            ] {
                let rows: Vec<&CellChange> = self
                    .moved
                    .iter()
                    .filter(|change: &&CellChange| change.key.metric == metric)
                    .collect();
                if rows.is_empty() {
                    continue;
                }
                let _ = write!(
                    out,
                    "\n<details><summary>{title} ({} cells)</summary>\n\n\
                     | Tier | Fixture | Operation | `{base_label}` | This PR | Change |\n\
                     |---|---|---|---|---|---|\n",
                    rows.len()
                );
                for change in rows {
                    let _ = writeln!(
                        out,
                        "| {} | {} | {} | {} | {} | {:+.1}% |",
                        change.key.tier.label(),
                        change.key.fixture,
                        change.key.operation.label(),
                        render(metric, &change.base),
                        render(metric, &change.head),
                        change.relative() * 100.0
                    );
                }
                out.push_str("\n</details>\n");
            }
        }
        for (cells, side) in [
            (&self.base_only, format!("only on `{base_label}`")),
            (&self.head_only, "only on this branch".to_owned()),
        ] {
            if cells.is_empty() {
                continue;
            }
            let _ = write!(out, "\nMeasured {side} ({} cells):", cells.len());
            for key in cells {
                let _ = write!(
                    out,
                    " {} {} {} {};",
                    key.tier.label(),
                    key.fixture,
                    key.operation.label(),
                    key.metric.label(key.tier)
                );
            }
            out.pop();
            out.push_str(".\n");
        }
        out
    }
}

/// A cell value as `median ± stddev` in the metric's unit.
fn render(metric: Metric, stats: &Stats) -> String {
    match metric {
        Metric::TimeSecs => format_time(stats.median, stats.stddev),
        Metric::PeakMemoryBytes => format_memory(stats.median, stats.stddev),
    }
}

#[cfg(test)]
mod tests {
    use super::{CellChange, Comparison, Samples, compare, moved_beyond_noise};
    use crate::{CellKey, Metric, Operation, Stats, Tier};

    fn key(fixture: &str, metric: Metric) -> CellKey {
        CellKey {
            tier: Tier::InProcess,
            operation: Operation::Codegen,
            metric,
            fixture: fixture.to_owned(),
        }
    }

    fn stats(median: f64, stddev: f64) -> Stats {
        Stats {
            median,
            stddev,
            min: median,
            max: median,
            samples: 2,
        }
    }

    fn samples(cells: &[(CellKey, Vec<f64>)]) -> Samples {
        cells.iter().cloned().collect()
    }

    #[test]
    fn moved_beyond_noise_when_the_gap_exceeds_the_threshold_and_the_spread() {
        let expected: bool = true;
        let actual: bool = moved_beyond_noise(&stats(100.0, 1.0), &stats(105.0, 1.0));
        assert_eq!(expected, actual);
    }

    #[test]
    fn moved_beyond_noise_is_false_within_the_threshold() {
        let expected: bool = false;
        let actual: bool = moved_beyond_noise(&stats(100.0, 0.0), &stats(102.9, 0.0));
        assert_eq!(expected, actual);
    }

    #[test]
    fn moved_beyond_noise_is_false_within_the_spread() {
        let expected: bool = false;
        let actual: bool = moved_beyond_noise(&stats(100.0, 3.0), &stats(105.0, 3.0));
        assert_eq!(expected, actual);
    }

    #[test]
    fn moved_beyond_noise_counts_an_improvement() {
        let expected: bool = true;
        let actual: bool = moved_beyond_noise(&stats(100.0, 0.0), &stats(90.0, 0.0));
        assert_eq!(expected, actual);
    }

    #[test]
    fn compare_sorts_moved_cells_best_first_and_counts_the_rest() {
        let base: Samples = samples(&[
            (key("a", Metric::TimeSecs), vec![10.0, 10.0]),
            (key("b", Metric::TimeSecs), vec![10.0, 10.0]),
            (key("c", Metric::TimeSecs), vec![10.0, 10.0]),
        ]);
        let head: Samples = samples(&[
            (key("a", Metric::TimeSecs), vec![12.0, 12.0]),
            (key("b", Metric::TimeSecs), vec![8.0, 8.0]),
            (key("c", Metric::TimeSecs), vec![10.1, 10.1]),
        ]);
        let expected: Comparison = Comparison {
            moved: vec![
                CellChange {
                    key: key("b", Metric::TimeSecs),
                    base: Stats::from_samples(&[10.0, 10.0]).unwrap(),
                    head: Stats::from_samples(&[8.0, 8.0]).unwrap(),
                },
                CellChange {
                    key: key("a", Metric::TimeSecs),
                    base: Stats::from_samples(&[10.0, 10.0]).unwrap(),
                    head: Stats::from_samples(&[12.0, 12.0]).unwrap(),
                },
            ],
            steady: 1,
            base_only: Vec::new(),
            head_only: Vec::new(),
        };
        let actual: Comparison = compare(&base, &head);
        assert_eq!(expected, actual);
    }

    #[test]
    fn compare_lists_cells_measured_on_one_side() {
        let base: Samples = samples(&[(key("gone", Metric::TimeSecs), vec![1.0])]);
        let head: Samples = samples(&[
            (key("new", Metric::TimeSecs), vec![1.0]),
            (key("empty", Metric::TimeSecs), Vec::new()),
        ]);
        let expected: Comparison = Comparison {
            moved: Vec::new(),
            steady: 0,
            base_only: vec![key("gone", Metric::TimeSecs)],
            head_only: vec![key("new", Metric::TimeSecs)],
        };
        let actual: Comparison = compare(&base, &head);
        assert_eq!(expected, actual);
    }

    #[test]
    fn to_markdown_says_nothing_moved() {
        let comparison: Comparison = Comparison {
            steady: 3,
            ..Comparison::default()
        };
        let expected: String = "Nothing moved beyond noise (3 cells of our library compared \
                                against `main`, measured in the same session).\n"
            .to_owned();
        let actual: String = comparison.to_markdown("main");
        assert_eq!(expected, actual);
    }

    #[test]
    fn to_markdown_tables_moved_cells_by_metric_and_lists_one_sided_cells() {
        let comparison: Comparison = Comparison {
            moved: vec![
                CellChange {
                    key: key("small/config", Metric::PeakMemoryBytes),
                    base: stats(2048.0, 0.0),
                    head: stats(1024.0, 0.0),
                },
                CellChange {
                    key: key("small/config", Metric::TimeSecs),
                    base: stats(0.001, 0.000_01),
                    head: stats(0.0012, 0.000_02),
                },
            ],
            steady: 4,
            base_only: vec![key("large/x", Metric::TimeSecs)],
            head_only: Vec::new(),
        };
        let expected: String = "2 of 6 cells of our library moved beyond noise against `main`, \
measured in the same session: 1 better, 1 worse.

<details><summary>Memory (1 cells)</summary>

| Tier | Fixture | Operation | `main` | This PR | Change |
|---|---|---|---|---|---|
| in-process | small/config | codegen | 2.00 KiB | 1.00 KiB | -50.0% |

</details>

<details><summary>Time (1 cells)</summary>

| Tier | Fixture | Operation | `main` | This PR | Change |
|---|---|---|---|---|---|
| in-process | small/config | codegen | 1.000 ± 0.010 ms | 1.200 ± 0.020 ms | +20.0% |

</details>

Measured only on `main` (1 cells): in-process large/x codegen time.
"
        .to_owned();
        let actual: String = comparison.to_markdown("main");
        assert_eq!(expected, actual);
    }
}
