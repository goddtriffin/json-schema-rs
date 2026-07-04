# Benchmark results

## Category winners

| Category | Winner |
|---|---|
| codegen wall-time | jsonschemars |
| schema-compile time | jsonschemars |
| validate (valid) | jsonschemars |
| validate (invalid) | jsonschemars |
| peak memory | jsonschemars |

## Detailed results

### codegen wall-time (microseconds; lower is better)

| Fixture | jsonschemars |
|---|---|
| criterion/nested | **75.051** |
| criterion/small | **16.599** |
| large/api-catalog | **10000.000** |
| large/circleci | **10000.000** |
| massive/data-warehouse | **30000.000** |
| massive/k8s-deployment | **60000.000** |
| medium/user-profile | **10000.000** |
| small/config | **10000.000** |

### schema-compile time (microseconds; lower is better)

| Fixture | jsonschemars |
|---|---|
| criterion/nested | **4.422** |
| criterion/small | **1.150** |

### validate (valid) (microseconds; lower is better)

| Fixture | boon | jsonschemars |
|---|---|---|
| criterion/nested | — | **6.504** |
| criterion/small | — | **0.692** |
| large/api-catalog | 10000.000 | **0.000** |
| large/circleci | 10000.000 | **0.000** |
| massive/data-warehouse | 90000.000 | **10000.000** |
| massive/k8s-deployment | 20000.000 | **10000.000** |
| medium/package-json | 10000.000 | **0.000** |
| medium/user-profile | 10000.000 | **0.000** |
| small/config | 10000.000 | **0.000** |
| small/prettierrc | 10000.000 | **0.000** |

### validate (invalid) (microseconds; lower is better)

| Fixture | boon | jsonschemars |
|---|---|---|
| criterion/nested | — | **5.805** |
| criterion/small | — | **0.836** |
| large/api-catalog | 10000.000 | **0.000** |
| large/circleci | 10000.000 | **0.000** |
| massive/data-warehouse | 90000.000 | **10000.000** |
| massive/k8s-deployment | 20000.000 | **10000.000** |
| medium/package-json | 10000.000 | **0.000** |
| medium/user-profile | 10000.000 | **0.000** |
| small/config | 10000.000 | **0.000** |
| small/prettierrc | 10000.000 | **0.000** |

### peak memory (bytes; lower is better)

| Fixture | boon | jsonschemars |
|---|---|---|
| large/api-catalog | 5734400 | **4603904** |
| large/circleci | **8847360** | 12746752 |
| massive/data-warehouse | **24543232** | 38338560 |
| massive/k8s-deployment | **11911168** | 100335616 |
| medium/package-json | 6209536 | **3440640** |
| medium/user-profile | 5521408 | **3604480** |
| small/config | 5537792 | **3096576** |
| small/prettierrc | 5652480 | **2949120** |

## Codegen compatibility

| Tool | Fixture | Status |
|---|---|---|
| jsonschemars | large/api-catalog | pass |
| jsonschemars | large/circleci | pass |
| jsonschemars | massive/data-warehouse | pass |
| jsonschemars | massive/k8s-deployment | pass |
| jsonschemars | medium/package-json | FAIL: codegen exited 1 |
| jsonschemars | medium/user-profile | pass |
| jsonschemars | small/config | pass |
| jsonschemars | small/prettierrc | FAIL: codegen exited 1 |
