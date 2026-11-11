# Description

Please include a summary of the change and which issue is fixed.
Please also include relevant motivation and context.

## JSON Schema spec parity

Official-suite tests that failed on `main` and pass with this PR (most relevant groups by keyword and draft, with counts; compliance/coverage change per draft from `json_schema_rs_differential/results.md`). `n/a` if nothing changed.

## Performance

Time and memory changes of our library versus `main`, from `make benchmark`'s report (both measured in the same session), counting only cells that moved beyond noise on code paths this PR changed: a short summary (biggest few with before → after, count, category-winner changes, why), then the full tables collapsed. One line, and no tables, if nothing moved beyond noise.

<details><summary>Memory (N cells)</summary>

| Tier | Fixture | Operation | `main` | This PR | Change |
|---|---|---|---|---|---|

</details>

<details><summary>Time (N cells)</summary>

| Tier | Fixture | Operation | `main` | This PR | Change |
|---|---|---|---|---|---|

</details>

Generated code (exact, so every change counts): the report's summary line and why the output changed, or its one line if nothing changed.

<details><summary>Generated code (N changes)</summary>

| Fixture | Settings | Metric | `main` | This PR | Change |
|---|---|---|---|---|---|

</details>
