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
| criterion/nested | **40.049** | — |
| criterion/small | **8.821** | — |
| large/api-catalog | **0.000** | 110000.000 |
| large/circleci | **0.000** | — |
| massive/data-warehouse | **10000.000** | 940000.000 |
| massive/k8s-deployment | **30000.000** | 100000.000 |
| medium/user-profile | **0.000** | 100000.000 |
| small/config | **0.000** | 100000.000 |

### schema-compile time (microseconds; lower is better)

| Fixture | jsonschemars |
|---|---|
| criterion/nested | **2.449** |
| criterion/small | **0.609** |

### validate (valid) (microseconds; lower is better)

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| criterion/nested | — | — | **3.419** |
| criterion/small | — | — | **0.383** |
| large/api-catalog | **0.000** | **0.000** | **0.000** |
| large/circleci | 10000.000 | **0.000** | **0.000** |
| massive/data-warehouse | 50000.000 | 10000.000 | **0.000** |
| massive/k8s-deployment | 10000.000 | 240000.000 | **0.000** |
| medium/package-json | — | 1240000.000 | **0.000** |
| medium/user-profile | **0.000** | **0.000** | **0.000** |
| small/config | **0.000** | **0.000** | **0.000** |
| small/prettierrc | **0.000** | **0.000** | **0.000** |

### validate (invalid) (microseconds; lower is better)

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| criterion/nested | — | — | **3.062** |
| criterion/small | — | — | **0.432** |
| large/api-catalog | **0.000** | **0.000** | **0.000** |
| large/circleci | 10000.000 | **0.000** | **0.000** |
| massive/data-warehouse | 50000.000 | 10000.000 | **0.000** |
| massive/k8s-deployment | 10000.000 | 220000.000 | **0.000** |
| medium/package-json | **0.000** | 1270000.000 | **0.000** |
| medium/user-profile | **0.000** | **0.000** | **0.000** |
| small/config | **0.000** | **0.000** | **0.000** |
| small/prettierrc | **0.000** | **0.000** | **0.000** |

### peak memory (bytes; lower is better)

| Fixture | boon | jsonschema-cli | jsonschemars | typify |
|---|---|---|---|---|
| large/api-catalog | 6062080 | 10829824 | **5144576** | 37175296 |
| large/circleci | **9568256** | 13041664 | 13320192 | — |
| massive/data-warehouse | 24969216 | **20758528** | 38748160 | 236224512 |
| massive/k8s-deployment | **12304384** | 20955136 | 100499456 | 33341440 |
| medium/package-json | — | 34111488 | **3817472** | — |
| medium/user-profile | 5947392 | 10616832 | **4014080** | 35635200 |
| small/config | 5898240 | 10649600 | **3489792** | 34553856 |
| small/prettierrc | 6012928 | 9076736 | **3276800** | — |

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
