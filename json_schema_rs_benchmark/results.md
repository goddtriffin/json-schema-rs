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
| generated/deep-nesting | **6.391 ± 0.031 ms** | 9.623 ± 0.033 ms |
| generated/large-array | **74.477 ± 0.086 µs** | 1.028 ± 0.002 ms |
| generated/many-defs | **37.841 ± 0.385 ms** | 121.837 ± 0.203 ms |
| generated/wide-object | **138.138 ± 0.989 ms** | 147.700 ± 0.262 ms |
| large/api-catalog | **208.510 ± 0.313 µs** | 3.084 ± 0.014 ms |
| large/circleci | 1.434 ± 0.005 ms | — |
| massive/data-warehouse | **9.582 ± 0.013 ms** | 268.502 ± 0.705 ms |
| massive/k8s-deployment | 19.390 ± 0.054 ms | **1.814 ± 0.026 ms** |
| medium/user-profile | **93.936 ± 0.108 µs** | 1.411 ± 0.001 ms |
| small/config | **29.798 ± 0.030 µs** | 512.545 ± 2.672 µs |

### codegen — peak heap

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | 37.79 MiB | **12.65 MiB** |
| generated/large-array | **534.31 KiB** | 1.23 MiB |
| generated/many-defs | **99.65 MiB** | 133.38 MiB |
| generated/wide-object | **33.33 MiB** | 162.36 MiB |
| large/api-catalog | **1.19 MiB** | 3.64 MiB |
| large/circleci | 8.34 MiB | — |
| massive/data-warehouse | **31.20 MiB** | 288.90 MiB |
| massive/k8s-deployment | 82.10 MiB | **2.79 MiB** |
| medium/user-profile | **481.48 KiB** | 1.71 MiB |
| small/config | **140.30 KiB** | 639.58 KiB |

### schema compile — time

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 1.017 ± 0.018 ms | 206.909 ± 0.253 µs | **63.262 ± 0.070 µs** |
| generated/large-array | 44.649 ± 0.043 µs | 20.911 ± 0.059 µs | **4.109 ± 0.006 µs** |
| generated/many-defs | 13.633 ± 0.030 ms | 3.022 ± 0.027 ms | **656.013 ± 10.282 µs** |
| generated/wide-object | 23.166 ± 0.052 ms | 2.537 ± 0.011 ms | **738.485 ± 1.175 µs** |
| large/api-catalog | 177.444 ± 0.289 µs | 103.529 ± 0.241 µs | **14.657 ± 0.025 µs** |
| large/circleci | 2.870 ± 0.012 ms | 1.026 ± 0.010 ms | **250.650 ± 0.259 µs** |
| massive/data-warehouse | 44.388 ± 0.515 ms | 6.339 ± 0.013 ms | **1.193 ± 0.001 ms** |
| massive/k8s-deployment | 7.285 ± 0.008 ms | 1.607 ± 0.004 ms | **595.993 ± 0.669 µs** |
| medium/user-profile | 66.803 ± 0.108 µs | 40.293 ± 0.032 µs | **5.358 ± 0.021 µs** |
| small/config | 26.788 ± 0.035 µs | 19.092 ± 0.057 µs | **1.776 ± 0.010 µs** |
| small/prettierrc | 203.904 ± 0.255 µs | 110.886 ± 0.386 µs | **24.091 ± 0.058 µs** |

### schema compile — peak heap

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 636.04 KiB | 534.41 KiB | **313.91 KiB** |
| generated/large-array | 66.67 KiB | 34.10 KiB | **32.81 KiB** |
| generated/many-defs | 8.10 MiB | **3.88 MiB** | 4.46 MiB |
| generated/wide-object | 13.73 MiB | 4.03 MiB | **2.85 MiB** |
| large/api-catalog | 266.71 KiB | 151.24 KiB | **92.01 KiB** |
| large/circleci | 3.85 MiB | 1.75 MiB | **754.94 KiB** |
| massive/data-warehouse | 16.73 MiB | 9.42 MiB | **3.10 MiB** |
| massive/k8s-deployment | 7.45 MiB | 3.28 MiB | **2.32 MiB** |
| medium/user-profile | 121.82 KiB | 59.13 KiB | **32.98 KiB** |
| small/config | 38.35 KiB | 24.08 KiB | **8.42 KiB** |
| small/prettierrc | 476.85 KiB | 261.04 KiB | **131.28 KiB** |

### validate (valid) — time

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.524 ± 0.015 µs | **2.495 ± 0.037 µs** | 139.226 ± 0.098 µs |
| generated/large-array | 18.007 ± 0.058 ms | **7.500 ± 0.077 ms** | 148.497 ± 0.189 ms |
| generated/many-defs | 1.717 ± 0.005 ms | **622.823 ± 3.606 µs** | 19.380 ± 0.053 ms |
| generated/wide-object | 170.043 ± 0.325 µs | **110.413 ± 0.573 µs** | 804.568 ± 3.986 µs |
| large/api-catalog | 564.141 ± 3.149 ns | **235.978 ± 1.172 ns** | 6.774 ± 0.022 µs |
| large/circleci | 2.216 ± 0.013 µs | **469.475 ± 3.241 ns** | 2.240 ± 0.010 µs |
| massive/data-warehouse | 69.311 ± 0.137 µs | **24.018 ± 0.042 µs** | 633.074 ± 2.077 µs |
| massive/k8s-deployment | 518.678 ± 1.447 ns | **218.586 ± 0.523 ns** | 4.634 ± 0.017 µs |
| medium/user-profile | 843.180 ± 2.697 ns | **395.630 ± 11.173 ns** | 5.913 ± 0.020 µs |
| small/config | 224.081 ± 1.431 ns | **114.796 ± 0.931 ns** | 2.797 ± 0.005 µs |
| small/prettierrc | 725.249 ± 3.513 ns | **181.752 ± 5.129 ns** | 3.210 ± 0.007 µs |

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
| generated/deep-nesting | 7.691 ± 0.015 µs | **2.883 ± 0.011 µs** | 141.199 ± 0.128 µs |
| generated/large-array | 18.146 ± 0.037 ms | **7.502 ± 0.054 ms** | 149.067 ± 1.044 ms |
| generated/many-defs | 1.721 ± 0.006 ms | **627.943 ± 1.893 µs** | 18.839 ± 0.045 ms |
| generated/wide-object | 171.411 ± 0.286 µs | **110.423 ± 0.591 µs** | 808.966 ± 9.593 µs |
| large/api-catalog | 664.450 ± 2.743 ns | **394.640 ± 0.720 ns** | 5.341 ± 0.011 µs |
| large/circleci | **126.480 ± 0.609 ns** | 131.646 ± 0.302 ns | 432.427 ± 0.703 ns |
| massive/data-warehouse | 182.245 ± 0.652 µs | **140.099 ± 0.494 µs** | 925.793 ± 2.224 µs |
| massive/k8s-deployment | 179.018 ± 0.454 ns | **137.788 ± 1.625 ns** | 579.056 ± 1.548 ns |
| medium/user-profile | 368.941 ± 1.386 ns | **318.496 ± 1.937 ns** | 2.418 ± 0.009 µs |
| small/config | 285.503 ± 0.425 ns | **242.227 ± 0.358 ns** | 2.824 ± 0.009 µs |
| small/prettierrc | **523.793 ± 1.694 ns** | 898.511 ± 2.629 ns | 919.320 ± 5.813 ns |

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
| generated/deep-nesting | **11.454 ± 0.462 ms** | 108.640 ± 1.161 ms |
| generated/large-array | **1.829 ± 0.043 ms** | 87.269 ± 0.718 ms |
| generated/many-defs | **45.068 ± 0.233 ms** | 442.764 ± 4.141 ms |
| generated/wide-object | **143.701 ± 1.391 ms** | 513.076 ± 3.031 ms |
| large/api-catalog | **2.205 ± 0.060 ms** | 94.254 ± 0.499 ms |
| large/circleci | 4.182 ± 0.061 ms | — |
| massive/data-warehouse | **14.256 ± 0.762 ms** | 789.878 ± 3.991 ms |
| massive/k8s-deployment | **28.229 ± 0.601 ms** | 87.731 ± 1.298 ms |
| medium/user-profile | **1.835 ± 0.029 ms** | 88.345 ± 0.758 ms |
| small/config | **1.721 ± 0.042 ms** | 85.725 ± 0.679 ms |

### codegen — peak RSS

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | 45.50 MiB | **38.00 ± 0.01 MiB** |
| generated/large-array | **3.95 MiB** | 33.30 ± 0.01 MiB |
| generated/many-defs | 115.97 ± 0.02 MiB | **98.05 ± 0.10 MiB** |
| generated/wide-object | **39.61 MiB** | 115.41 ± 0.53 MiB |
| large/api-catalog | **5.03 ± 0.01 MiB** | 34.69 ± 0.03 MiB |
| large/circleci | 13.22 MiB | — |
| massive/data-warehouse | **39.08 MiB** | 176.88 ± 0.05 MiB |
| massive/k8s-deployment | 97.05 MiB | **31.80 ± 0.01 MiB** |
| medium/user-profile | **3.94 MiB** | 33.58 ± 0.01 MiB |
| small/config | **3.50 ± 0.01 MiB** | 32.94 ± 0.01 MiB |

### validate (valid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.531 ± 0.475 ms † | 4.210 ± 0.100 ms | **1.922 ± 0.089 ms** |
| generated/large-array | 58.874 ± 0.265 ms | **44.977 ± 1.055 ms** | 188.031 ± 1.172 ms |
| generated/many-defs | 23.052 ± 0.223 ms | **11.361 ± 0.103 ms** | 26.324 ± 0.288 ms |
| generated/wide-object | 27.829 ± 0.229 ms | 8.045 ± 0.115 ms | **4.098 ± 0.046 ms** |
| large/api-catalog | 3.105 ± 0.092 ms | 3.972 ± 0.081 ms | **1.624 ± 0.056 ms** |
| large/circleci | 6.837 ± 0.073 ms | 5.716 ± 0.095 ms | **2.016 ± 0.033 ms** |
| massive/data-warehouse | 49.853 ± 0.375 ms | 11.940 ± 0.156 ms | **4.311 ± 0.067 ms** |
| massive/k8s-deployment | 11.965 ± 0.232 ms | 264.756 ± 39.761 ms † | **2.693 ± 0.055 ms** |
| medium/user-profile | 2.962 ± 0.096 ms | 3.691 ± 0.151 ms | **1.562 ± 0.029 ms** |
| small/config | 2.847 ± 0.087 ms | 3.657 ± 0.183 ms | **1.517 ± 0.027 ms** |
| small/prettierrc | 3.131 ± 0.130 ms | 3.770 ± 0.095 ms | **1.606 ± 0.077 ms** |

### validate (valid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.89 ± 0.02 MiB | 9.89 MiB | **4.12 ± 0.01 MiB** |
| generated/large-array | **55.81 MiB** | 64.67 MiB | 68.42 MiB |
| generated/many-defs | 19.08 MiB | 21.36 ± 0.01 MiB | **14.42 MiB** |
| generated/wide-object | 15.12 ± 0.01 MiB | 15.28 ± 0.01 MiB | **7.23 MiB** |
| large/api-catalog | 5.84 ± 0.01 MiB | 10.36 MiB | **3.36 ± 0.01 MiB** |
| large/circleci | 9.17 MiB | 13.06 ± 0.02 MiB | **4.41 MiB** |
| massive/data-warehouse | 22.02 MiB | 20.98 ± 0.01 MiB | **7.92 ± 0.00 MiB** |
| massive/k8s-deployment | 11.91 MiB | 20.52 MiB | **6.45 ± 0.01 MiB** |
| medium/user-profile | 5.70 MiB | 10.09 ± 0.01 MiB | **3.28 MiB** |
| small/config | 5.66 ± 0.01 MiB | 9.84 ± 0.01 MiB | **3.19 MiB** |
| small/prettierrc | 5.81 ± 0.01 MiB | 8.95 ± 0.01 MiB | **3.30 MiB** |

### validate (invalid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.117 ± 0.136 ms | 4.077 ± 0.063 ms | **1.949 ± 0.045 ms** |
| generated/large-array | 59.035 ± 0.384 ms | **44.836 ± 6.940 ms †** | 188.893 ± 1.079 ms |
| generated/many-defs | 23.269 ± 0.419 ms | **11.638 ± 0.297 ms** | 26.265 ± 0.254 ms |
| generated/wide-object | 27.811 ± 0.121 ms | 8.082 ± 0.108 ms | **4.131 ± 0.228 ms** |
| large/api-catalog | 3.101 ± 0.156 ms | 3.943 ± 0.050 ms | **1.575 ± 0.041 ms** |
| large/circleci | 6.833 ± 0.146 ms | 5.609 ± 0.147 ms | **1.985 ± 0.034 ms** |
| massive/data-warehouse | 51.101 ± 0.268 ms | 11.843 ± 0.107 ms | **4.575 ± 0.204 ms** |
| massive/k8s-deployment | 11.884 ± 0.091 ms | 238.006 ± 29.088 ms † | **2.708 ± 0.077 ms** |
| medium/user-profile | 2.931 ± 0.125 ms | 3.831 ± 0.153 ms | **1.529 ± 0.033 ms** |
| small/config | 2.828 ± 0.046 ms | 3.619 ± 0.059 ms | **1.510 ± 0.033 ms** |
| small/prettierrc | 3.088 ± 0.143 ms | 3.764 ± 0.075 ms | **1.549 ± 0.039 ms** |

### validate (invalid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.89 ± 0.02 MiB | 10.00 ± 0.02 MiB | **4.22 ± 0.01 MiB** |
| generated/large-array | **55.84 ± 0.02 MiB** | 64.80 MiB | 68.48 ± 0.01 MiB |
| generated/many-defs | 20.78 MiB | 21.45 ± 0.01 MiB | **14.45 MiB** |
| generated/wide-object | 15.12 MiB | 15.42 ± 0.02 MiB | **7.27 ± 0.01 MiB** |
| large/api-catalog | 5.84 ± 0.01 MiB | 10.50 MiB | **3.36 MiB** |
| large/circleci | 9.17 ± 0.03 MiB | 13.08 ± 0.04 MiB | **4.42 ± 0.01 MiB** |
| massive/data-warehouse | 22.62 ± 0.05 MiB | 21.03 ± 0.03 MiB | **7.72 ± 0.02 MiB** |
| massive/k8s-deployment | 11.83 ± 0.02 MiB | 20.52 ± 0.02 MiB | **6.44 ± 0.00 MiB** |
| medium/user-profile | 5.70 ± 0.01 MiB | 10.20 ± 0.02 MiB | **3.27 ± 0.01 MiB** |
| small/config | 5.67 ± 0.01 MiB | 10.02 MiB | **3.22 MiB** |
| small/prettierrc | 5.81 MiB | 9.14 ± 0.01 MiB | **3.30 MiB** |

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
