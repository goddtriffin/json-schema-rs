//! The quick benchmark's comparison (`make benchmark`): our tool's cells measured on a base
//! commit and on the working tree, in the same session, and which of them moved beyond noise.
//!
//! The driver (`scripts/quick_benchmark.sh`) builds both sides, measures them cell by cell in
//! interleaved rounds (base, head, head, base), each round its own process, and collects every
//! raw sample per cell, side and round into [`Samples`]; the `compare` binary reads those
//! captures, calls [`compare`], and renders [`Comparison::to_markdown`], the PR body's
//! *Performance* section.
//!
//! Each side of a cell is summarized by an [`Estimate`] built in two steps, one per kind of
//! noise measured in #100 on a shared machine:
//!
//! - **Within a round, the lowest sample.** Load from other processes (builds, benchmarks,
//!   sessions) only ever adds time or memory, and it arrives in bursts of seconds: between two
//!   runs of the same binary a cell's median moved by up to 17%, far beyond the standard
//!   deviation of either run's own samples. The lowest sample is the one no burst reached.
//! - **Across rounds, the median, with their standard deviation as the spread.** Each process
//!   gets its own memory layout, and some sub-microsecond cells run about 4% faster in some
//!   processes than in others, so one lucky process must not stand for a whole side.
//!
//! A cell moved when the two sides' estimates differ by more than [`TIE_THRESHOLD`] of the
//! base's **and** by more than the sum of both spreads.

use crate::{CellKey, Metric, Stats, TIE_THRESHOLD, Tier, format_memory, format_time};
use std::collections::BTreeMap;
use std::fmt::Write as _;

/// Every raw sample of one side, by cell and round (one inner list per round, in capture
/// order): seconds per call or run for time, bytes for memory.
pub type Samples = BTreeMap<CellKey, Vec<Vec<f64>>>;

/// Add one round's `values` to the samples of `key`.
pub fn add_round(samples: &mut Samples, key: CellKey, values: impl IntoIterator<Item = f64>) {
    samples
        .entry(key)
        .or_default()
        .push(values.into_iter().collect());
}

/// One side's summary of a cell: the median of its rounds' lowest samples, and how far those
/// spread.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Estimate {
    /// The median of the rounds' lowest samples.
    pub value: f64,
    /// The standard deviation of the rounds' lowest samples (zero for a single round).
    pub spread: f64,
}

impl Estimate {
    /// Summarize a side's rounds; `None` when no round has a sample.
    #[must_use]
    pub fn from_rounds(rounds: &[Vec<f64>]) -> Option<Self> {
        let lowest: Vec<f64> = rounds
            .iter()
            .filter_map(|round: &Vec<f64>| round.iter().copied().min_by(f64::total_cmp))
            .collect();
        let stats: Stats = Stats::from_samples(&lowest)?;
        Some(Self {
            value: stats.median,
            spread: stats.stddev,
        })
    }
}

/// One cell measured on both sides.
#[derive(Debug, Clone, PartialEq)]
pub struct CellChange {
    /// The cell.
    pub key: CellKey,
    /// The base side's samples, summarized.
    pub base: Estimate,
    /// The head side's samples, summarized.
    pub head: Estimate,
}

impl CellChange {
    /// The head's estimate relative to the base's (`-0.1` is 10% lower, so better). Zero when
    /// the base's estimate is zero.
    #[must_use]
    pub fn relative(&self) -> f64 {
        if self.base.value == 0.0 {
            return 0.0;
        }
        (self.head.value - self.base.value) / self.base.value
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

/// Whether `head` differs from `base` beyond noise: the estimates differ by more than
/// [`TIE_THRESHOLD`] of the base's and by more than the sum of both spreads.
#[must_use]
pub fn moved_beyond_noise(base: &Estimate, head: &Estimate) -> bool {
    let gap: f64 = (head.value - base.value).abs();
    gap > (TIE_THRESHOLD * base.value).max(base.spread + head.spread)
}

/// Compare every cell of both sides. A cell with no samples on a side counts as not measured
/// there.
#[must_use]
pub fn compare(base: &Samples, head: &Samples) -> Comparison {
    let mut comparison: Comparison = Comparison::default();
    for (key, base_rounds) in base {
        let Some(base_estimate) = Estimate::from_rounds(base_rounds) else {
            continue;
        };
        let Some(head_estimate) = head
            .get(key)
            .and_then(|rounds| Estimate::from_rounds(rounds))
        else {
            comparison.base_only.push(key.clone());
            continue;
        };
        if moved_beyond_noise(&base_estimate, &head_estimate) {
            comparison.moved.push(CellChange {
                key: key.clone(),
                base: base_estimate,
                head: head_estimate,
            });
        } else {
            comparison.steady += 1;
        }
    }
    let measured =
        |rounds: &Vec<Vec<f64>>| -> bool { rounds.iter().any(|round| !round.is_empty()) };
    for (key, head_rounds) in head {
        if measured(head_rounds) && !base.get(key).is_some_and(measured) {
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
    /// The moved cells of `tier`, as `<operation>/<fixture id>` (the operation's token), each
    /// once whatever metrics moved, sorted. The driver re-measures these in confirmation rounds.
    #[must_use]
    pub fn moved_cells(&self, tier: Tier) -> Vec<String> {
        let mut cells: Vec<String> = self
            .moved
            .iter()
            .filter(|change: &&CellChange| change.key.tier == tier)
            .map(|change: &CellChange| {
                format!("{}/{}", change.key.operation.token(), change.key.fixture)
            })
            .collect();
        cells.sort();
        cells.dedup();
        cells
    }

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
                 `{base_label}`: interleaved in the same session, per-round minimums)."
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
                 `{base_label}` (interleaved in the same session, per-round minimums): \
                 {better} better, {} worse.",
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

/// A side of a cell as `estimate ± spread` in the metric's unit.
fn render(metric: Metric, estimate: &Estimate) -> String {
    match metric {
        Metric::TimeSecs => format_time(estimate.value, estimate.spread),
        Metric::PeakMemoryBytes => format_memory(estimate.value, estimate.spread),
    }
}

#[cfg(test)]
mod tests {
    use super::{CellChange, Comparison, Estimate, Samples, compare, moved_beyond_noise};
    use crate::{CellKey, Metric, Operation, Tier};

    fn key(fixture: &str, metric: Metric) -> CellKey {
        CellKey {
            tier: Tier::InProcess,
            operation: Operation::Codegen,
            metric,
            fixture: fixture.to_owned(),
        }
    }

    fn estimate(value: f64, spread: f64) -> Estimate {
        Estimate { value, spread }
    }

    fn samples(cells: &[(CellKey, Vec<Vec<f64>>)]) -> Samples {
        cells.iter().cloned().collect()
    }

    fn steady(count: usize) -> Comparison {
        Comparison {
            steady: count,
            ..Comparison::default()
        }
    }

    #[test]
    fn estimate_is_the_median_of_the_round_minimums_and_their_spread() {
        let expected: Option<Estimate> = Some(estimate(2.0, 1.0));
        let actual: Option<Estimate> =
            Estimate::from_rounds(&[vec![4.0, 1.0], vec![3.0, 5.0], vec![2.0, 9.0]]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn estimate_of_one_round_has_no_spread() {
        let expected: Option<Estimate> = Some(estimate(4.0, 0.0));
        let actual: Option<Estimate> = Estimate::from_rounds(&[vec![6.0, 4.0]]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn estimate_skips_an_empty_round() {
        let expected: Option<Estimate> = Some(estimate(4.0, 0.0));
        let actual: Option<Estimate> = Estimate::from_rounds(&[Vec::new(), vec![4.0]]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn estimate_of_no_samples_is_none() {
        let expected: Option<Estimate> = None;
        let actual: Option<Estimate> = Estimate::from_rounds(&[Vec::new()]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn compare_ignores_a_burst_that_slowed_some_samples_of_each_round() {
        let base: Samples = samples(&[(
            key("a", Metric::TimeSecs),
            vec![vec![10.0, 10.1], vec![10.0, 10.1]],
        )]);
        let head: Samples = samples(&[(
            key("a", Metric::TimeSecs),
            vec![vec![10.0, 14.0], vec![13.0, 10.1]],
        )]);
        let expected: Comparison = steady(1);
        let actual: Comparison = compare(&base, &head);
        assert_eq!(expected, actual);
    }

    #[test]
    fn compare_ignores_one_slow_round_of_two() {
        let base: Samples = samples(&[(
            key("a", Metric::TimeSecs),
            vec![vec![10.0, 10.1], vec![10.0, 10.1]],
        )]);
        let head: Samples = samples(&[(
            key("a", Metric::TimeSecs),
            vec![vec![13.0, 14.0], vec![10.0, 10.1]],
        )]);
        let expected: Comparison = steady(1);
        let actual: Comparison = compare(&base, &head);
        assert_eq!(expected, actual);
    }

    #[test]
    fn moved_beyond_noise_when_the_gap_exceeds_the_threshold_and_the_spread() {
        let expected: bool = true;
        let actual: bool = moved_beyond_noise(&estimate(100.0, 1.0), &estimate(105.0, 1.0));
        assert_eq!(expected, actual);
    }

    #[test]
    fn moved_beyond_noise_is_false_within_the_threshold() {
        let expected: bool = false;
        let actual: bool = moved_beyond_noise(&estimate(100.0, 0.0), &estimate(102.9, 0.0));
        assert_eq!(expected, actual);
    }

    #[test]
    fn moved_beyond_noise_is_false_within_the_spread() {
        let expected: bool = false;
        let actual: bool = moved_beyond_noise(&estimate(100.0, 3.0), &estimate(105.0, 3.0));
        assert_eq!(expected, actual);
    }

    #[test]
    fn moved_beyond_noise_counts_an_improvement() {
        let expected: bool = true;
        let actual: bool = moved_beyond_noise(&estimate(100.0, 0.0), &estimate(90.0, 0.0));
        assert_eq!(expected, actual);
    }

    #[test]
    fn compare_sorts_moved_cells_best_first_and_counts_the_rest() {
        let base: Samples = samples(&[
            (
                key("a", Metric::TimeSecs),
                vec![vec![10.0, 11.0], vec![10.0]],
            ),
            (
                key("b", Metric::TimeSecs),
                vec![vec![10.0, 11.0], vec![10.0]],
            ),
            (
                key("c", Metric::TimeSecs),
                vec![vec![10.0, 11.0], vec![10.0]],
            ),
        ]);
        let head: Samples = samples(&[
            (
                key("a", Metric::TimeSecs),
                vec![vec![12.0, 13.0], vec![12.0]],
            ),
            (key("b", Metric::TimeSecs), vec![vec![8.0, 9.0], vec![8.0]]),
            (
                key("c", Metric::TimeSecs),
                vec![vec![10.1, 11.0], vec![10.1]],
            ),
        ]);
        let expected: Comparison = Comparison {
            moved: vec![
                CellChange {
                    key: key("b", Metric::TimeSecs),
                    base: estimate(10.0, 0.0),
                    head: estimate(8.0, 0.0),
                },
                CellChange {
                    key: key("a", Metric::TimeSecs),
                    base: estimate(10.0, 0.0),
                    head: estimate(12.0, 0.0),
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
        let base: Samples = samples(&[(key("gone", Metric::TimeSecs), vec![vec![1.0]])]);
        let head: Samples = samples(&[
            (key("new", Metric::TimeSecs), vec![vec![1.0]]),
            (key("empty", Metric::TimeSecs), vec![Vec::new()]),
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
    fn moved_cells_lists_each_cell_of_the_tier_once() {
        let cli_key: CellKey = CellKey {
            tier: Tier::Cli,
            ..key("small/config", Metric::TimeSecs)
        };
        let comparison: Comparison = Comparison {
            moved: vec![
                CellChange {
                    key: key("small/config", Metric::TimeSecs),
                    base: estimate(1.0, 0.0),
                    head: estimate(2.0, 0.0),
                },
                CellChange {
                    key: key("large/api-catalog", Metric::TimeSecs),
                    base: estimate(1.0, 0.0),
                    head: estimate(2.0, 0.0),
                },
                CellChange {
                    key: key("small/config", Metric::PeakMemoryBytes),
                    base: estimate(1.0, 0.0),
                    head: estimate(2.0, 0.0),
                },
                CellChange {
                    key: cli_key,
                    base: estimate(1.0, 0.0),
                    head: estimate(2.0, 0.0),
                },
            ],
            ..Comparison::default()
        };
        let expected: Vec<String> = vec![
            "codegen/large/api-catalog".to_owned(),
            "codegen/small/config".to_owned(),
        ];
        let actual: Vec<String> = comparison.moved_cells(Tier::InProcess);
        assert_eq!(expected, actual);
    }

    #[test]
    fn to_markdown_says_nothing_moved() {
        let comparison: Comparison = steady(3);
        let expected: String = "Nothing moved beyond noise (3 cells of our library compared \
                                against `main`: interleaved in the same session, per-round \
                                minimums).\n"
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
                    base: estimate(2048.0, 0.0),
                    head: estimate(1024.0, 0.0),
                },
                CellChange {
                    key: key("small/config", Metric::TimeSecs),
                    base: estimate(0.001, 0.000_01),
                    head: estimate(0.0012, 0.000_02),
                },
            ],
            steady: 4,
            base_only: vec![key("large/x", Metric::TimeSecs)],
            head_only: Vec::new(),
        };
        let expected: String = "2 of 6 cells of our library moved beyond noise against `main` \
(interleaved in the same session, per-round minimums): 1 better, 1 worse.

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
