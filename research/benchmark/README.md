# Benchmarking (design)

**Purpose**: Compare performance of our generator and each competitor on the same set of JSON Schema inputs. Measure wall time (e.g. via [Hyperfine](https://github.com/sharkdp/hyperfine)) so we can avoid regressions and stay best-in-class.

**Layout**:

- **fixtures/** — JSON Schema files used for benchmarking (small, medium, large). Language-agnostic; the same schemas are run through each tool. Fixtures may come from vendored spec examples or our tests. No fixtures are added in the initial phase; add them when implementing the harness.

**Tools to measure** (when implemented):

- Our crate (e.g. `json-schema-to-rust-cli` or `make … input=… output=…`)
- schemafy (e.g. `cargo typify` or schemafy’s CLI)
- typify (e.g. `cargo typify …`)
- Other competitors per `research/repos/<lang>/`

**Implementation**: Exact commands and runner scripts (e.g. shell scripts or a small harness that invokes each tool on each fixture, then runs Hyperfine) will be added when benchmarks are implemented. Until then, use `make research-benchmark` as a stub that reports “Not implemented”.
