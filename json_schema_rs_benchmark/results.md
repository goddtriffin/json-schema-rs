# Benchmark results

## Category winners

> Winners count outright per-cell wins only. Time categories also include our in-process criterion cells (our library only), and many cross-tool CLI cells round to `0.000` under `/usr/bin/time`'s ~10 ms resolution (counted as ties, not wins), so a time-category winner can reflect criterion-only cells rather than head-to-head speed. Peak memory is measured head-to-head across tools. See issue #21.

| Category | Winner |
|---|---|
| codegen wall-time | jsonschemars |
| schema-compile time | jsonschemars |
| validate (valid) | jsonschemars |
| validate (invalid) | jsonschemars |
| peak memory | jsonschemars |

## Detailed results

### codegen wall-time (microseconds; lower is better)

| Fixture | jsonschemars | typify |
|---|---|---|
| criterion/nested | **39.888** | — |
| criterion/small | **8.818** | — |
| large/api-catalog | **0.000** | 120000.000 |
| large/circleci | **0.000** | — |
| massive/data-warehouse | **10000.000** | 950000.000 |
| massive/k8s-deployment | **30000.000** | 110000.000 |
| medium/user-profile | **0.000** | 120000.000 |
| small/config | **0.000** | 110000.000 |

### schema-compile time (microseconds; lower is better)

| Fixture | jsonschemars |
|---|---|
| criterion/nested | **2.438** |
| criterion/small | **0.611** |

### validate (valid) (microseconds; lower is better)

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| criterion/nested | — | — | **3.470** |
| criterion/small | — | — | **0.376** |
| large/api-catalog | **0.000** | **0.000** | **0.000** |
| large/circleci | 10000.000 | **0.000** | **0.000** |
| massive/data-warehouse | 50000.000 | 10000.000 | **0.000** |
| massive/k8s-deployment | 10000.000 | 230000.000 | **0.000** |
| medium/package-json | — | 1290000.000 | **0.000** |
| medium/user-profile | **0.000** | **0.000** | **0.000** |
| small/config | **0.000** | **0.000** | **0.000** |
| small/prettierrc | **0.000** | **0.000** | **0.000** |

### validate (invalid) (microseconds; lower is better)

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| criterion/nested | — | — | **3.064** |
| criterion/small | — | — | **0.432** |
| large/api-catalog | **0.000** | **0.000** | **0.000** |
| large/circleci | 10000.000 | **0.000** | **0.000** |
| massive/data-warehouse | 50000.000 | 10000.000 | **0.000** |
| massive/k8s-deployment | 10000.000 | 220000.000 | **0.000** |
| medium/package-json | **0.000** | 1290000.000 | **0.000** |
| medium/user-profile | **0.000** | **0.000** | **0.000** |
| small/config | **0.000** | **0.000** | **0.000** |
| small/prettierrc | **0.000** | **0.000** | **0.000** |

### peak memory (bytes; lower is better)

| Fixture | boon | jsonschema-cli | jsonschemars | typify |
|---|---|---|---|---|
| large/api-catalog | 6111232 | 10813440 | **5144576** | 37175296 |
| large/circleci | **9568256** | 13008896 | 13320192 | — |
| massive/data-warehouse | 25018368 | **20791296** | 38748160 | 236306432 |
| massive/k8s-deployment | **12304384** | 21102592 | 100646912 | 33341440 |
| medium/package-json | — | 34258944 | **3817472** | — |
| medium/user-profile | 6062080 | 10649600 | **3981312** | 35553280 |
| small/config | 5898240 | 10633216 | **3489792** | 34586624 |
| small/prettierrc | 6029312 | 9076736 | **3276800** | — |

## Codegen compatibility

| Tool | Fixture | Status |
|---|---|---|
| jsonschemars | large/api-catalog | pass |
| typify | large/api-catalog | pass |
| jsonschemars | large/circleci | pass |
| typify | large/circleci | FAIL: codegen exited 101 |
| jsonschemars | massive/data-warehouse | pass |
| typify | massive/data-warehouse | pass |
| jsonschemars | massive/k8s-deployment | pass |
| typify | massive/k8s-deployment | pass |
| jsonschemars | medium/package-json | FAIL: codegen exited 1 |
| typify | medium/package-json | FAIL: codegen exited 101 |
| jsonschemars | medium/user-profile | pass |
| typify | medium/user-profile | pass |
| jsonschemars | small/config | pass |
| typify | small/config | pass |
| jsonschemars | small/prettierrc | FAIL: codegen exited 1 |
| typify | small/prettierrc | FAIL: codegen exited 1 |
