# json-schema-rs-differential

Differential-testing harness that runs the `json-schema-rs` validator alongside other JSON
Schema implementations on the official
[JSON Schema Test Suite](https://github.com/json-schema-org/JSON-Schema-Test-Suite) and on the
benchmark fixtures, using [Bowtie](https://github.com/bowtie-json-schema/bowtie). It reports how
often each implementation passes, where our verdicts differ from the majority, which keywords we
do not support yet, and how our error messages compare with competitors'. This crate is a
workspace member and is **not published** (`publish = false`).

## Run it

Prerequisites (the driver checks each one before it starts):

```sh
uv tool install bowtie-json-schema   # the Bowtie CLI
# a running container engine (Docker Desktop, or another engine Bowtie supports)
make vendor_test_suite                # clone the official test suite
```

Then, from the repo root:

```sh
make differential
```

The run builds our harness image, pulls the competitor images on first use, runs every draft,
and writes:

- `results.md`: the summary, the per-draft compliance tables, our coverage and correctness,
  unsupported keywords ranked by the number of tests they affect, disagreements with the
  majority, fixture verdicts, and the error-message comparison. **Committed.**
- `results.json`: the same data, machine-readable. **Committed.**
- `reports/`: the raw Bowtie JSONL reports and their logs. **Gitignored.**

Settings (environment variables):

| Variable | Effect |
|---|---|
| `DIFF_DRAFTS="draft7 draft2020-12"` | Run only the drafts you list. The default is all five; the report builder needs all five. |
| `DIFF_SKIP_IMAGE_BUILD=1` | Reuse the existing `localhost/json-schema-rs:latest` image. |
| `DIFF_SKIP_BOWTIE=1` | Skip Bowtie and rebuild `results.{json,md}` from the existing `reports/`. |
| `DIFF_BOWTIE_TIMEOUT=900` | Seconds before one Bowtie call is treated as hung and the run fails. |

## How it works

| Piece | Role |
|---|---|
| `src/bin/bowtie_harness.rs` | Our Bowtie harness. It reads Bowtie's protocol on stdin and validates with `json-schema-rs`, parsing strictly. When it can't handle a schema it returns a skip with the reason, not a verdict. |
| `Dockerfile` (+ `Dockerfile.dockerignore`) | Builds the harness into `localhost/json-schema-rs:latest` from the repo root. Bowtie runs it as `-i image:localhost/json-schema-rs`. |
| `scripts/run_differential.sh` | The driver: prerequisite checks, image build, `bowtie suite` per draft (main and optional tests), and `bowtie run` per fixture (its resource documents in the case registry). The competitor list is `COMPETITORS` here. |
| `src/bin/differential_report.rs` + `src/*.rs` | Parse the Bowtie reports and work out compliance, coverage and correctness, and the majority vote. Also compare error messages in-process against the `jsonschema` and `boon` crates (Bowtie only reports valid/invalid, not messages). Writes `results.{json,md}`. |

The design and its reasoning are in [design.md](../design.md) under
"Differential-testing harness".
