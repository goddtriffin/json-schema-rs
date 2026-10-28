# Benchmark results

Two tiers, lower is better everywhere. **In-process**: every tool is linked as a Rust library into one process; time is criterion's median ± standard deviation per call, and peak heap is the extra heap the operation allocates, recorded by a counting allocator. **CLI**: every tool's binary runs end to end (process startup, file I/O, and the operation), timed by hyperfine as median ± standard deviation over repeated runs after warmup; peak RSS is the process's peak resident set size. A cell is measured only when the tool is correct on it (see Compatibility). A cell's best tool wins outright (**bold**) only when at least two tools were measured and its lead over the runner-up exceeds both 3% of its median and the sum of their standard deviations; otherwise the cell is a tie or uncontested and nothing is bolded. `†` marks a measurement whose standard deviation exceeds 10% of its median.

## Tool versions

| Tool | Version |
|---|---|
| boon | 0.6.1 |
| cargo-typify | 0.8.0 |
| hyperfine | 1.20.0 |
| json-schema-rs | 0.0.6 |
| jsonschema | 0.60.2 |
| jsonschema-cli | 0.60.2 |
| rustc | 1.99.1 |
| typify | 0.8.0 |

## Category winners

| Tier | Operation | Metric | Winner | Outright wins / contested |
|---|---|---|---|---|
| in-process | codegen | time | jsonschemars | 8 / 9 |
| in-process | codegen | peak heap | jsonschemars | 7 / 9 |
| in-process | schema compile | time | jsonschemars | 11 / 11 |
| in-process | schema compile | peak heap | jsonschemars | 10 / 11 |
| in-process | validate (valid) | time | jsonschema | 11 / 11 |
| in-process | validate (valid) | peak heap | jsonschema | 11 / 11 |
| in-process | validate (invalid) | time | jsonschema | 9 / 11 |
| in-process | validate (invalid) | peak heap | jsonschema | 9 / 11 |
| CLI | codegen | time | jsonschemars | 9 / 9 |
| CLI | codegen | peak RSS | jsonschemars | 6 / 9 |
| CLI | validate (valid) | time | jsonschemars | 9 / 11 |
| CLI | validate (valid) | peak RSS | jsonschemars | 10 / 11 |
| CLI | validate (invalid) | time | jsonschemars | 9 / 11 |
| CLI | validate (invalid) | peak RSS | jsonschemars | 10 / 11 |

## In-process (library calls)

### codegen — time

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | **6.403 ± 0.050 ms** | 9.692 ± 0.111 ms |
| generated/large-array | **73.384 ± 0.309 µs** | 1.030 ± 0.001 ms |
| generated/many-defs | **37.099 ± 0.066 ms** | 121.619 ± 1.876 ms |
| generated/wide-object | **136.394 ± 1.727 ms** | 146.530 ± 0.405 ms |
| large/api-catalog | **206.249 ± 0.284 µs** | 3.099 ± 0.009 ms |
| large/circleci | 1.387 ± 0.002 ms | — |
| massive/data-warehouse | **9.401 ± 0.036 ms** | 267.623 ± 0.563 ms |
| massive/k8s-deployment | 18.586 ± 0.098 ms | **1.819 ± 0.007 ms** |
| medium/user-profile | **93.123 ± 0.514 µs** | 1.423 ± 0.008 ms |
| small/config | **29.457 ± 0.044 µs** | 510.232 ± 0.752 µs |

### codegen — peak heap

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | 36.00 MiB | **12.65 MiB** |
| generated/large-array | **503.70 KiB** | 1.23 MiB |
| generated/many-defs | **93.09 MiB** | 133.38 MiB |
| generated/wide-object | **31.36 MiB** | 162.36 MiB |
| large/api-catalog | **1.12 MiB** | 3.64 MiB |
| large/circleci | 7.80 MiB | — |
| massive/data-warehouse | **29.33 MiB** | 288.90 MiB |
| massive/k8s-deployment | 78.20 MiB | **2.79 MiB** |
| medium/user-profile | **453.59 KiB** | 1.71 MiB |
| small/config | **133.13 KiB** | 639.58 KiB |

### schema compile — time

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 1.018 ± 0.018 ms | 206.637 ± 2.033 µs | **64.503 ± 0.168 µs** |
| generated/large-array | 44.932 ± 0.111 µs | 20.725 ± 0.027 µs | **4.345 ± 0.011 µs** |
| generated/many-defs | 13.624 ± 0.015 ms | 3.017 ± 0.006 ms | **712.097 ± 9.674 µs** |
| generated/wide-object | 23.198 ± 0.177 ms | 2.528 ± 0.005 ms | **770.558 ± 0.767 µs** |
| large/api-catalog | 177.787 ± 0.221 µs | 103.876 ± 0.108 µs | **15.694 ± 0.031 µs** |
| large/circleci | 2.852 ± 0.015 ms | 1.018 ± 0.004 ms | **255.362 ± 0.568 µs** |
| massive/data-warehouse | 44.052 ± 0.074 ms | 6.270 ± 0.008 ms | **1.276 ± 0.002 ms** |
| massive/k8s-deployment | 7.287 ± 0.022 ms | 1.596 ± 0.003 ms | **608.146 ± 0.495 µs** |
| medium/user-profile | 66.827 ± 0.070 µs | 39.990 ± 0.077 µs | **5.675 ± 0.010 µs** |
| small/config | 26.925 ± 0.042 µs | 19.080 ± 0.061 µs | **1.841 ± 0.005 µs** |
| small/prettierrc | 204.741 ± 0.357 µs | 111.106 ± 0.118 µs | **25.505 ± 0.034 µs** |

### schema compile — peak heap

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 636.04 KiB | 534.41 KiB | **293.28 KiB** |
| generated/large-array | 66.67 KiB | 34.10 KiB | **30.66 KiB** |
| generated/many-defs | 8.10 MiB | **3.88 MiB** | 4.16 MiB |
| generated/wide-object | 13.73 MiB | 4.03 MiB | **2.67 MiB** |
| large/api-catalog | 266.71 KiB | 151.24 KiB | **86.06 KiB** |
| large/circleci | 3.85 MiB | 1.75 MiB | **704.83 KiB** |
| massive/data-warehouse | 16.73 MiB | 9.42 MiB | **2.91 MiB** |
| massive/k8s-deployment | 7.45 MiB | 3.28 MiB | **2.18 MiB** |
| medium/user-profile | 121.82 KiB | 59.13 KiB | **30.83 KiB** |
| small/config | 38.35 KiB | 24.08 KiB | **7.90 KiB** |
| small/prettierrc | 476.85 KiB | 261.04 KiB | **123.03 KiB** |

### validate (valid) — time

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.299 ± 0.012 µs | **2.453 ± 0.007 µs** | 139.214 ± 0.473 µs |
| generated/large-array | 17.983 ± 0.063 ms | **7.239 ± 0.034 ms** | 146.667 ± 0.373 ms |
| generated/many-defs | 1.712 ± 0.005 ms | **616.801 ± 1.462 µs** | 18.903 ± 0.028 ms |
| generated/wide-object | 169.916 ± 0.521 µs | **112.625 ± 1.034 µs** | 787.406 ± 1.925 µs |
| large/api-catalog | 563.204 ± 1.444 ns | **240.437 ± 2.849 ns** | 6.687 ± 0.019 µs |
| large/circleci | 2.167 ± 0.019 µs | **465.029 ± 1.944 ns** | 2.155 ± 0.006 µs |
| massive/data-warehouse | 69.778 ± 0.239 µs | **23.738 ± 0.110 µs** | 608.250 ± 15.510 µs |
| massive/k8s-deployment | 526.676 ± 3.212 ns | **219.123 ± 1.943 ns** | 4.626 ± 0.013 µs |
| medium/user-profile | 847.888 ± 6.269 ns | **376.080 ± 7.137 ns** | 5.842 ± 0.109 µs |
| small/config | 224.166 ± 1.742 ns | **113.323 ± 0.874 ns** | 2.792 ± 0.035 µs |
| small/prettierrc | 721.092 ± 4.311 ns | **176.975 ± 3.247 ns** | 3.104 ± 0.021 µs |

### validate (valid) — peak heap

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 2.25 KiB | **0.03 KiB** | 11.74 KiB |
| generated/large-array | 0.19 KiB | **0.03 KiB** | 8.47 MiB |
| generated/many-defs | 0.56 KiB | **0.03 KiB** | 16.86 KiB |
| generated/wide-object | 0.19 KiB | **0.03 KiB** | 548.52 KiB |
| large/api-catalog | 0.19 KiB | **0.03 KiB** | 2.95 KiB |
| large/circleci | 2.58 KiB | **0.03 KiB** | 1.25 KiB |
| massive/data-warehouse | 0.19 KiB | **0.03 KiB** | 14.77 KiB |
| massive/k8s-deployment | 0.19 KiB | **0.03 KiB** | 1.56 KiB |
| medium/user-profile | 0.19 KiB | **0.03 KiB** | 3.48 KiB |
| small/config | 0.19 KiB | **0.03 KiB** | 2.96 KiB |
| small/prettierrc | 0.77 KiB | **0.03 KiB** | 3.78 KiB |

### validate (invalid) — time

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 7.417 ± 0.032 µs | **2.808 ± 0.018 µs** | 140.476 ± 1.745 µs |
| generated/large-array | 18.388 ± 0.172 ms | **7.326 ± 0.081 ms** | 149.245 ± 1.251 ms |
| generated/many-defs | 1.758 ± 0.047 ms | **624.638 ± 1.830 µs** | 19.065 ± 0.126 ms |
| generated/wide-object | 171.714 ± 0.711 µs | **112.964 ± 3.422 µs** | 800.730 ± 10.424 µs |
| large/api-catalog | 653.834 ± 2.466 ns | **395.302 ± 5.800 ns** | 5.395 ± 0.024 µs |
| large/circleci | **125.836 ± 0.553 ns** | 129.957 ± 0.659 ns | 402.173 ± 1.938 ns |
| massive/data-warehouse | 178.976 ± 1.267 µs | **139.307 ± 0.438 µs** | 894.587 ± 9.049 µs |
| massive/k8s-deployment | 176.138 ± 0.740 ns | **134.095 ± 0.476 ns** | 560.347 ± 2.238 ns |
| medium/user-profile | 366.306 ± 1.029 ns | **310.559 ± 0.727 ns** | 2.382 ± 0.011 µs |
| small/config | 285.353 ± 0.932 ns | **244.349 ± 0.403 ns** | 2.771 ± 0.008 µs |
| small/prettierrc | **517.415 ± 1.469 ns** | 898.175 ± 3.740 ns | 866.744 ± 3.740 ns |

### validate (invalid) — peak heap

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 2.97 KiB | **0.75 KiB** | 11.74 KiB |
| generated/large-array | 1.37 KiB | **0.46 KiB** | 8.47 MiB |
| generated/many-defs | 7.77 KiB | **0.77 KiB** | 16.86 KiB |
| generated/wide-object | 0.74 KiB | **0.41 KiB** | 548.52 KiB |
| large/api-catalog | 1.44 KiB | **0.83 KiB** | 3.12 KiB |
| large/circleci | 0.74 KiB | **0.40 KiB** | 1.22 KiB |
| massive/data-warehouse | 467.89 KiB | 111.78 KiB | **4.59 KiB** |
| massive/k8s-deployment | 0.81 KiB | **0.48 KiB** | 1.07 KiB |
| medium/user-profile | 1.30 KiB | **0.73 KiB** | 1.85 KiB |
| small/config | 1.30 KiB | **0.71 KiB** | 4.06 KiB |
| small/prettierrc | 2.38 KiB | 2.69 KiB | **1.55 KiB** |

## CLI (end to end)

### codegen — time

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | **11.044 ± 0.211 ms** | 108.107 ± 0.325 ms |
| generated/large-array | **1.820 ± 0.088 ms** | 87.236 ± 0.751 ms |
| generated/many-defs | **44.321 ± 0.327 ms** | 442.668 ± 2.712 ms |
| generated/wide-object | **149.699 ± 1.548 ms** | 511.268 ± 2.313 ms |
| large/api-catalog | **2.127 ± 0.057 ms** | 94.015 ± 0.497 ms |
| large/circleci | 4.029 ± 0.111 ms | — |
| massive/data-warehouse | **13.986 ± 0.172 ms** | 788.885 ± 3.676 ms |
| massive/k8s-deployment | **27.876 ± 0.516 ms** | 86.337 ± 1.056 ms |
| medium/user-profile | **1.832 ± 0.045 ms** | 87.811 ± 1.727 ms |
| small/config | **1.691 ± 0.041 ms** | 85.316 ± 0.589 ms |

### codegen — peak RSS

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | 45.45 MiB | **38.02 ± 0.02 MiB** |
| generated/large-array | **3.78 ± 0.01 MiB** | 33.31 MiB |
| generated/many-defs | 115.06 MiB | **98.19 ± 0.09 MiB** |
| generated/wide-object | **38.67 ± 0.01 MiB** | 115.45 ± 0.15 MiB |
| large/api-catalog | **4.89 MiB** | 34.67 ± 0.03 MiB |
| large/circleci | 12.67 MiB | — |
| massive/data-warehouse | **36.97 MiB** | 176.84 ± 0.00 MiB |
| massive/k8s-deployment | 95.98 MiB | **31.80 MiB** |
| medium/user-profile | **3.78 ± 0.01 MiB** | 33.56 MiB |
| small/config | **3.34 MiB** | 32.98 MiB |

### validate (valid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.406 ± 0.421 ms | 4.122 ± 0.231 ms | **1.897 ± 0.028 ms** |
| generated/large-array | 58.712 ± 0.400 ms | **44.656 ± 0.856 ms** | 188.860 ± 1.979 ms |
| generated/many-defs | 22.938 ± 0.288 ms | **11.518 ± 0.208 ms** | 26.124 ± 0.259 ms |
| generated/wide-object | 27.870 ± 0.200 ms | 8.001 ± 0.086 ms | **4.033 ± 0.041 ms** |
| large/api-catalog | 3.103 ± 0.078 ms | 3.967 ± 0.104 ms | **1.604 ± 0.070 ms** |
| large/circleci | 6.846 ± 0.156 ms | 5.700 ± 0.102 ms | **1.995 ± 0.028 ms** |
| massive/data-warehouse | 49.801 ± 0.307 ms | 11.837 ± 0.078 ms | **4.213 ± 0.048 ms** |
| massive/k8s-deployment | 11.931 ± 0.913 ms | 444.796 ± 622.446 ms † | **2.605 ± 0.053 ms** |
| medium/user-profile | 2.864 ± 0.074 ms | 3.700 ± 0.147 ms | **1.546 ± 0.031 ms** |
| small/config | 2.817 ± 0.079 ms | 3.591 ± 0.178 ms | **1.494 ± 0.029 ms** |
| small/prettierrc | 3.142 ± 0.106 ms | 3.768 ± 0.092 ms | **1.598 ± 0.054 ms** |

### validate (valid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.89 ± 0.01 MiB | 9.89 MiB | **4.00 MiB** |
| generated/large-array | **55.78 MiB** | 64.69 ± 0.01 MiB | 68.28 ± 0.03 MiB |
| generated/many-defs | 19.08 MiB | 21.36 ± 0.23 MiB | **14.27 MiB** |
| generated/wide-object | 15.09 ± 0.01 MiB | 15.28 ± 0.01 MiB | **6.97 ± 0.00 MiB** |
| large/api-catalog | 5.86 ± 0.00 MiB | 10.36 ± 0.01 MiB | **3.11 ± 0.01 MiB** |
| large/circleci | 9.22 MiB | 13.06 MiB | **4.25 ± 0.02 MiB** |
| massive/data-warehouse | 23.56 MiB | 20.95 ± 0.01 MiB | **7.53 ± 0.01 MiB** |
| massive/k8s-deployment | 11.77 ± 0.32 MiB | 20.42 ± 0.05 MiB | **6.20 ± 0.01 MiB** |
| medium/user-profile | 5.72 MiB | 10.05 ± 0.01 MiB | **3.02 MiB** |
| small/config | 5.69 ± 0.01 MiB | 9.86 ± 0.01 MiB | **2.95 MiB** |
| small/prettierrc | 5.88 ± 0.01 MiB | 8.95 MiB | **3.14 ± 0.01 MiB** |

### validate (invalid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.124 ± 0.098 ms | 4.030 ± 0.051 ms | **1.920 ± 0.103 ms** |
| generated/large-array | 58.822 ± 0.546 ms | **44.878 ± 0.652 ms** | 188.035 ± 10.844 ms |
| generated/many-defs | 23.102 ± 0.243 ms | **11.523 ± 0.153 ms** | 26.535 ± 0.858 ms |
| generated/wide-object | 27.800 ± 0.095 ms | 8.089 ± 0.081 ms | **4.066 ± 0.116 ms** |
| large/api-catalog | 3.072 ± 0.152 ms | 3.933 ± 0.073 ms | **1.548 ± 0.032 ms** |
| large/circleci | 6.834 ± 0.054 ms | 5.696 ± 0.167 ms | **1.973 ± 0.023 ms** |
| massive/data-warehouse | 51.253 ± 0.294 ms | 11.864 ± 0.110 ms | **4.470 ± 0.211 ms** |
| massive/k8s-deployment | 11.853 ± 0.154 ms | 438.031 ± 164.496 ms † | **2.645 ± 0.076 ms** |
| medium/user-profile | 2.857 ± 0.078 ms | 3.755 ± 0.178 ms | **1.514 ± 0.028 ms** |
| small/config | 2.868 ± 0.085 ms | 3.591 ± 0.077 ms | **1.486 ± 0.029 ms** |
| small/prettierrc | 3.052 ± 0.193 ms | 3.740 ± 0.063 ms | **1.531 ± 0.019 ms** |

### validate (invalid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.91 MiB | 10.02 MiB | **4.02 MiB** |
| generated/large-array | **55.78 MiB** | 64.80 MiB | 68.25 ± 0.02 MiB |
| generated/many-defs | 19.06 ± 0.01 MiB | 21.42 ± 0.01 MiB | **14.50 ± 0.08 MiB** |
| generated/wide-object | 15.11 MiB | 15.39 ± 0.01 MiB | **7.00 ± 0.00 MiB** |
| large/api-catalog | 5.91 ± 0.01 MiB | 10.50 MiB | **3.12 MiB** |
| large/circleci | 9.19 ± 0.02 MiB | 13.08 ± 0.02 MiB | **4.23 ± 0.02 MiB** |
| massive/data-warehouse | 22.59 ± 0.02 MiB | 21.06 MiB | **7.36 ± 0.04 MiB** |
| massive/k8s-deployment | 11.77 ± 0.03 MiB | 20.47 ± 0.04 MiB | **6.20 ± 0.01 MiB** |
| medium/user-profile | 5.70 ± 0.01 MiB | 10.20 ± 0.00 MiB | **3.02 MiB** |
| small/config | 5.70 MiB | 10.02 MiB | **2.98 ± 0.01 MiB** |
| small/prettierrc | 5.88 MiB | 9.14 ± 0.01 MiB | **3.11 MiB** |

## Compatibility

203 of 228 attempted cells pass. Rows with at least one failure:

| Tier | Operation | Fixture | boon | jsonschema | jsonschema-cli | jsonschemars | typify |
|---|---|---|---|---|---|---|---|
| in-process | codegen | large/circleci | — | — | — | pass | FAIL: panic: not implemented: if/then/else schemas are not supported: { "if": { "required": [ "type" ], "properties": { "type": { "const": "release" } } }, "then": { "required": [ "plan_name" ], "properties": { "plan_name": { "type": "string", "markdownDescription": "Required plan name for release jobs." } }, "additionalProperties": true, "markdownDescription": "https://circleci.com/docs/reference/configuration-reference/#job-type\n\nRelease job." }, "else": { "if": { "required": [ "type" ], "properties": { "type": { "enum": [ "lock", "unlock" ] } } }, "then": { "required": [ "key" ], "properties": { "key": { "type": "string", "markdownDescription": "Required key for lock/unlock jobs." } }, "additionalProperties": true, "markdownDescription": "https://circleci.com/docs/reference/configuration-reference/#job-type\n\nA lock/unlock job" }, "else": { "if": { "required": [ "type" ], "properties": { "type": { "enum": [ "approval", "no-op" ] } } }, "then": { "additionalProperties": true, "markdownDescription": "https://circleci.com/docs/reference/configuration-reference/#job-type\n\nApproval/no-op jobs primarily act as workflow markers. Other keys like `steps:` are permitted but will be ignored in the final pipeline." }, "else": { "if": { "anyOf": [ { "not": { "required": [ "type" ] } }, { "required": [ "type" ], "properties": { "type": { "const": "build" } } } ], "markdownDescription": "https://circleci.com/docs/reference/configuration-reference/#job-type\n\nA build job. The job explicitly has type 'build' or defaults to build when no type specified." }, "then": { "anyOf": [ { "type": "object", "required": [ "executor" ], "markdownDescription": "A job that uses an executor defined elsewhere in the configuration." }, { "oneOf": [ { "type": "object", "required": [ "machine" ] }, { "type": "object", "required": [ "docker" ] }, { "type": "object", "required": [ "macos" ] } ] } ], "required": [ "steps" ], "properties": { "circleci_ip_ranges": { "type": "boolean", "markdownDescription": "Enable running jobs with a set of well-defined IP address ranges." }, "description": { "type": "string" }, "docker": { "type": "array", "items": { "type": "object", "required": [ "image" ], "properties": { "auth": { "type": "object", "required": [ "password", "username" ], "properties": { "password": { "type": "string" }, "username": { "type": "string" } }, "additionalProperties": false }, "aws_auth": { "oneOf": [ { "type": "object", "required": [ "aws_access_key_id", "aws_secret_access_key" ], "properties": { "aws_access_key_id": { "type": "string" }, "aws_secret_access_key": { "type": "string" } }, "additionalProperties": false }, { "type": "object", "required": [ "oidc_role_arn" ], "properties": { "oidc_role_arn": { "type": "string" } }, "additionalProperties": false } ] }, "command": { "oneOf": [ { "type": "string" }, { "type": "array", "items": { "type": "string" } } ] }, "entrypoint": { "oneOf": [ { "type": "string" }, { "type": "array", "items": { "type": "string" } } ] }, "environment": { "oneOf": [ { "type": [ "string", "null" ], "markdownDescription": "A map of environment variable names and values." }, { "type": "array", "items": { "$ref": "#/definitions/environment" }, "markdownDescription": "An array of strings in the form KEY=VALUE" }, { "type": "object", "additionalProperties": { "type": [ "string", "boolean", "number", "null" ] }, "markdownDescription": "The value should be a string, but we allow null, boolean and numbers too. Examples: `FOO: true` and `BAR: 2` `BAZ:`" } ] }, "image": { "type": "string" }, "name": { "type": "string" }, "user": { "type": "string" } }, "additionalProperties": false }, "minItems": 1 }, "environment": { "oneOf": [ { "type": [ "string", "null" ], "markdownDescription": "A map of environment variable names and values." }, { "type": "array", "items": { "$ref": "#/definitions/environment" }, "markdownDescription": "An array of strings in the form KEY=VALUE" }, { "type": "object", "additionalProperties": { "type": [ "string", "boolean", "number", "null" ] }, "markdownDescription": "The value should be a string, but we allow null, boolean and numbers too. Examples: `FOO: true` and `BAR: 2` `BAZ:`" } ] }, "executor": { "oneOf": [ { "type": "string", "markdownDescription": "https://circleci.com/docs/reference/reusing-config/#executor\n\nThe name of the executor to use (defined via the top level executors map or from an orb)." }, { "type": "object", "minProperties": 1, "required": [ "name" ], "properties": { "name": { "type": "string" } }, "additionalProperties": true, "markdownDescription": "Executor stanza to use for the job, with optional parameter overrides." } ] }, "machine": { "oneOf": [ { "anyOf": [ { "type": "string", "pattern": " *<< *parameters.([^ ]+) *>> *" }, { "type": "boolean" }, { "type": "number" }, { "type": "string" } ] }, { "type": "object", "properties": { "docker_layer_caching": { "anyOf": [ { "type": "string", "pattern": " *<< *parameters.([^ ]+) *>> *" }, { "type": "boolean" }, { "type": "number" }, { "type": "string" } ], "markdownDescription": "Set to `true` to enable [Docker Layer Caching](https://circleci.com/docs/docker-layer-caching). Note: If you haven't already, you must open a support ticket to have a CircleCI Sales representative contact you about enabling this feature on your account for an additional fee." }, "enabled": { "anyOf": [ { "type": "string", "pattern": " *<< *parameters.([^ ]+) *>> *" }, { "type": "boolean" }, { "type": "number" }, { "type": "string" } ], "markdownDescription": "A boolean or a template parameter evaluating to a boolean" }, "image": { "type": "string" }, "resource_class": { "type": "string" }, "shell": { "type": "string" } }, "additionalProperties": false } ], "markdownDescription": "Use the default machine executor image" }, "macos": { "type": "object", "required": [ "xcode" ], "properties": { "resource_class": { "type": "string" }, "shell": { "type": "string" }, "xcode": { "type": [ "string", "number" ] } }, "additionalProperties": false }, "parallelism": { "anyOf": [ { "oneOf": [ { "type": "string", "pattern": " *<< *parameters.([^ ]+) *>> *" }, { "type": "string", "pattern": " *<< *pipeline.parameters.([^ ]+) *>> *" } ] }, { "type": "integer" } ], "markdownDescription": "A integer or a parameter evaluating to a integer" }, "parameters": { "type": "object", "minProperties": 1, "additionalProperties": { "type": "object", "required": [ "type" ], "properties": { "default": { "oneOf": [ { "type": "string" }, { "type": "boolean" }, { "type": "integer" }, { "type": "array", "items": { "$ref": "#/definitions/step" } } ] }, "description": { "type": "string", "markdownDescription": "A description of the parameter" }, "enum": { "type": "array", "items": { "type": "string" }, "minItems": 1, "markdownDescription": "List of allowed values for an `enum` type parameter" }, "type": { "type": "string", "enum": [ "boolean", "string", "steps", "enum", "executor", "integer", "env_var_name" ], "markdownDescription": "The parameter type. See [Parameter Types](https://circleci.com/docs/reusing-config#parameter-syntax) for details:\n- `boolean`: A boolean parameter\n- `string`: A string parameter \n- `integer`: An integer parameter\n- `enum`: An enum parameter (list of allowed string values)\n- `executor`: An executor parameter (allows job to choose executor)\n- `steps`: A steps parameter (allows mixing predefined and user-defined steps)\n- `env_var_name`: An environment variable name parameter (must match POSIX_NAME regexp)" } }, "additionalProperties": false }, "markdownDescription": "https://circleci.com/docs/reusing-config#using-the-parameters-declaration\n\nA map of parameter keys." }, "resource_class": { "type": "string" }, "retention": { "type": "object", "properties": { "caches": { "type": "string", "pattern": "^([1-9]\|1[0-5])d$" } }, "additionalProperties": false }, "shell": { "oneOf": [ { "type": "string" }, { "type": "array", "items": { "type": "string" } } ] }, "steps": { "type": "array", "items": { "$ref": "#/definitions/step" }, "minItems": 1 }, "type": { "const": "build", "markdownDescription": "The job type. If not specified, defaults to build." }, "working_directory": { "type": "string", "markdownDescription": "In which directory to run the steps. (default: `~/project`. `project` is a literal string, not the name of the project.) You can also refer the directory with `$CIRCLE_WORKING_DIRECTORY` environment variable." } }, "additionalProperties": false, "markdownDescription": "Validation: The job must adhere to the structure of a build job." }, "else": false } } } } |
| in-process | codegen | medium/package-json | — | — | — | FAIL: not measured: schema needs external documents (offline run) | FAIL: not measured: schema needs external documents (offline run) |
| in-process | codegen | small/prettierrc | — | — | — | FAIL: codegen error: schema at index 0: allOf subschema at index 0 is not object-like (need type "object" or non-empty properties) | FAIL: codegen error: value does not conform to the given schema |
| in-process | schema compile | medium/package-json | FAIL: not measured: schema needs external documents (offline run) | FAIL: not measured: schema needs external documents (offline run) | — | FAIL: not measured: schema needs external documents (offline run) | — |
| in-process | validate (valid) | medium/package-json | FAIL: not measured: schema needs external documents (offline run) | FAIL: not measured: schema needs external documents (offline run) | — | FAIL: not measured: schema needs external documents (offline run) | — |
| in-process | validate (invalid) | medium/package-json | FAIL: not measured: schema needs external documents (offline run) | FAIL: not measured: schema needs external documents (offline run) | — | FAIL: not measured: schema needs external documents (offline run) | — |
| CLI | codegen | large/circleci | — | — | — | pass | FAIL: exit 101: not implemented: if/then/else schemas are not supported: { |
| CLI | codegen | medium/package-json | — | — | — | FAIL: not measured: schema needs external documents (offline run) | FAIL: not measured: schema needs external documents (offline run) |
| CLI | codegen | small/prettierrc | — | — | — | FAIL: exit 1: error: schema at index 0: allOf subschema at index 0 is not object-like (need type "object" or non-empty properties) | FAIL: exit 1: value does not conform to the given schema |
| CLI | validate (valid) | medium/package-json | FAIL: not measured: schema needs external documents (offline run) | — | FAIL: not measured: schema needs external documents (offline run) | FAIL: not measured: schema needs external documents (offline run) | — |
| CLI | validate (invalid) | medium/package-json | FAIL: not measured: schema needs external documents (offline run) | — | FAIL: not measured: schema needs external documents (offline run) | FAIL: not measured: schema needs external documents (offline run) | — |
