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

| Fixture | jsonschemars | typify |
|---|---|---|
| criterion/nested | **41.415** | — |
| criterion/small | **9.037** | — |
| large/api-catalog | **0.000** | 50000.000 |
| large/circleci | **0.000** | — |
| massive/data-warehouse | **10000.000** | 890000.000 |
| massive/k8s-deployment | **30000.000** | 40000.000 |
| medium/user-profile | **0.000** | 40000.000 |
| small/config | **0.000** | 40000.000 |

### schema-compile time (microseconds; lower is better)

| Fixture | jsonschemars |
|---|---|
| criterion/nested | **2.373** |
| criterion/small | **0.608** |

### validate (valid) (microseconds; lower is better)

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| criterion/nested | — | — | **3.524** |
| criterion/small | — | — | **0.368** |
| large/api-catalog | **0.000** | **0.000** | **0.000** |
| large/circleci | **0.000** | **0.000** | **0.000** |
| massive/data-warehouse | 50000.000 | 10000.000 | **0.000** |
| massive/k8s-deployment | 10000.000 | 380000.000 | **0.000** |
| medium/package-json | **0.000** | 2040000.000 | **0.000** |
| medium/user-profile | **0.000** | **0.000** | **0.000** |
| small/config | **0.000** | **0.000** | **0.000** |
| small/prettierrc | **0.000** | **0.000** | **0.000** |

### validate (invalid) (microseconds; lower is better)

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| criterion/nested | — | — | **3.182** |
| criterion/small | — | — | **0.456** |
| large/api-catalog | **0.000** | **0.000** | **0.000** |
| large/circleci | **0.000** | **0.000** | **0.000** |
| massive/data-warehouse | 50000.000 | 10000.000 | **0.000** |
| massive/k8s-deployment | 10000.000 | 360000.000 | **0.000** |
| medium/package-json | **0.000** | 2080000.000 | **0.000** |
| medium/user-profile | **0.000** | **0.000** | **0.000** |
| small/config | **0.000** | **0.000** | **0.000** |
| small/prettierrc | **0.000** | **0.000** | **0.000** |

### peak memory (bytes; lower is better)

| Fixture | boon | jsonschema-cli | jsonschemars | typify |
|---|---|---|---|---|
| large/api-catalog | 5718016 | 11141120 | **4603904** | 34930688 |
| large/circleci | **8798208** | 13139968 | 12779520 | — |
| massive/data-warehouse | 22659072 | **21020672** | 38322176 | 240041984 |
| massive/k8s-deployment | **11911168** | 20824064 | 100040704 | 31064064 |
| medium/package-json | 6193152 | 33701888 | **3440640** | — |
| medium/user-profile | 5521408 | 10928128 | **3604480** | 33357824 |
| small/config | 5537792 | 10928128 | **3080192** | 32292864 |
| small/prettierrc | 5652480 | 9273344 | **2949120** | — |

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
