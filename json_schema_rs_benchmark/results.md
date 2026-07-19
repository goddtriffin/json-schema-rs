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
| criterion/nested | **44.090** | — |
| criterion/small | **9.714** | — |
| large/api-catalog | **0.000** | 80000.000 |
| large/circleci | **10000.000** | — |
| massive/data-warehouse | **20000.000** | 970000.000 |
| massive/k8s-deployment | **30000.000** | 70000.000 |
| medium/user-profile | **0.000** | 60000.000 |
| small/config | **0.000** | 60000.000 |

### schema-compile time (microseconds; lower is better)

| Fixture | jsonschemars |
|---|---|
| criterion/nested | **2.581** |
| criterion/small | **0.660** |

### validate (valid) (microseconds; lower is better)

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| criterion/nested | — | — | **3.944** |
| criterion/small | — | — | **0.410** |
| large/api-catalog | **0.000** | **0.000** | **0.000** |
| large/circleci | 10000.000 | 10000.000 | **0.000** |
| massive/data-warehouse | 50000.000 | 20000.000 | **0.000** |
| massive/k8s-deployment | 10000.000 | 230000.000 | **0.000** |
| medium/package-json | — | 1950000.000 | **0.000** |
| medium/user-profile | **0.000** | **0.000** | **0.000** |
| small/config | **0.000** | **0.000** | **0.000** |
| small/prettierrc | **0.000** | **0.000** | **0.000** |

### validate (invalid) (microseconds; lower is better)

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| criterion/nested | — | — | **3.497** |
| criterion/small | — | — | **0.500** |
| large/api-catalog | **0.000** | **0.000** | **0.000** |
| large/circleci | 10000.000 | 10000.000 | **0.000** |
| massive/data-warehouse | 50000.000 | 10000.000 | **0.000** |
| massive/k8s-deployment | 10000.000 | 220000.000 | **0.000** |
| medium/package-json | **0.000** | 1920000.000 | **0.000** |
| medium/user-profile | **0.000** | **0.000** | **0.000** |
| small/config | **0.000** | **0.000** | **0.000** |
| small/prettierrc | **0.000** | **0.000** | **0.000** |

### peak memory (bytes; lower is better)

| Fixture | boon | jsonschema-cli | jsonschemars | typify |
|---|---|---|---|---|
| large/api-catalog | 5701632 | 11157504 | **4685824** | 35045376 |
| large/circleci | **9043968** | 13287424 | 12894208 | — |
| massive/data-warehouse | 22593536 | **21102592** | 38322176 | 240697344 |
| massive/k8s-deployment | **11894784** | 20660224 | 100171776 | 31309824 |
| medium/package-json | — | 33816576 | **3440640** | — |
| medium/user-profile | 5554176 | 10960896 | **3588096** | 33374208 |
| small/config | 5488640 | 10928128 | **3047424** | 32489472 |
| small/prettierrc | 5636096 | 9322496 | **2916352** | — |

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
