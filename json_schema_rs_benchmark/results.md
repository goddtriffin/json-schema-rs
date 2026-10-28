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
| generated/deep-nesting | **6.274 ± 0.091 ms** | 9.710 ± 0.222 ms |
| generated/large-array | **73.276 ± 0.143 µs** | 1.036 ± 0.005 ms |
| generated/many-defs | **37.389 ± 0.360 ms** | 122.716 ± 0.340 ms |
| generated/wide-object | **139.645 ± 1.228 ms** | 147.441 ± 1.669 ms |
| large/api-catalog | **207.488 ± 0.995 µs** | 3.108 ± 0.011 ms |
| large/circleci | 1.397 ± 0.010 ms | — |
| massive/data-warehouse | **9.503 ± 0.104 ms** | 268.248 ± 2.094 ms |
| massive/k8s-deployment | 18.707 ± 0.430 ms | **1.819 ± 0.008 ms** |
| medium/user-profile | **92.904 ± 0.231 µs** | 1.427 ± 0.008 ms |
| small/config | **29.740 ± 0.088 µs** | 518.387 ± 1.493 µs |

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
| generated/deep-nesting | 1.032 ± 0.016 ms | 207.683 ± 0.523 µs | **64.747 ± 0.524 µs** |
| generated/large-array | 45.273 ± 0.171 µs | 20.981 ± 0.044 µs | **4.356 ± 0.017 µs** |
| generated/many-defs | 13.790 ± 0.112 ms | 3.029 ± 0.011 ms | **712.539 ± 12.322 µs** |
| generated/wide-object | 23.414 ± 0.312 ms | 2.529 ± 0.012 ms | **772.693 ± 2.455 µs** |
| large/api-catalog | 180.459 ± 0.745 µs | 104.225 ± 0.297 µs | **15.727 ± 0.048 µs** |
| large/circleci | 2.894 ± 0.013 ms | 1.022 ± 0.008 ms | **257.042 ± 0.669 µs** |
| massive/data-warehouse | 45.408 ± 0.415 ms | 6.368 ± 0.083 ms | **1.275 ± 0.014 ms** |
| massive/k8s-deployment | 7.393 ± 0.051 ms | 1.598 ± 0.006 ms | **615.201 ± 3.283 µs** |
| medium/user-profile | 67.361 ± 0.371 µs | 40.408 ± 0.179 µs | **5.724 ± 0.021 µs** |
| small/config | 27.282 ± 0.174 µs | 19.170 ± 0.067 µs | **1.857 ± 0.009 µs** |
| small/prettierrc | 206.935 ± 1.745 µs | 112.362 ± 0.226 µs | **25.472 ± 0.118 µs** |

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
| generated/deep-nesting | 6.757 ± 0.013 µs | **2.438 ± 0.006 µs** | 139.987 ± 0.284 µs |
| generated/large-array | 18.089 ± 0.126 ms | **7.431 ± 0.074 ms** | 147.642 ± 1.316 ms |
| generated/many-defs | 1.759 ± 0.013 ms | **621.868 ± 1.923 µs** | 18.981 ± 0.413 ms |
| generated/wide-object | 168.805 ± 1.137 µs | **111.549 ± 1.388 µs** | 787.291 ± 3.069 µs |
| large/api-catalog | 568.775 ± 3.137 ns | **244.507 ± 4.503 ns** | 6.808 ± 0.030 µs |
| large/circleci | 2.222 ± 0.016 µs | **470.854 ± 4.924 ns** | 2.190 ± 0.010 µs |
| massive/data-warehouse | 69.942 ± 0.319 µs | **23.869 ± 0.184 µs** | 611.782 ± 2.974 µs |
| massive/k8s-deployment | 524.382 ± 1.329 ns | **216.629 ± 0.644 ns** | 4.641 ± 0.019 µs |
| medium/user-profile | 853.662 ± 2.982 ns | **376.934 ± 8.456 ns** | 5.994 ± 0.051 µs |
| small/config | 228.762 ± 1.285 ns | **112.730 ± 0.423 ns** | 2.789 ± 0.020 µs |
| small/prettierrc | 737.167 ± 11.770 ns | **175.788 ± 0.996 ns** | 3.045 ± 0.015 µs |

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
| generated/deep-nesting | 7.677 ± 0.020 µs | **2.811 ± 0.031 µs** | 141.551 ± 0.394 µs |
| generated/large-array | 18.232 ± 0.159 ms | **7.462 ± 0.216 ms** | 147.443 ± 0.829 ms |
| generated/many-defs | 1.764 ± 0.008 ms | **624.095 ± 8.914 µs** | 19.148 ± 0.184 ms |
| generated/wide-object | 169.438 ± 0.660 µs | **111.050 ± 2.261 µs** | 788.476 ± 5.731 µs |
| large/api-catalog | 666.746 ± 2.506 ns | **397.175 ± 6.031 ns** | 5.345 ± 0.037 µs |
| large/circleci | 132.839 ± 0.248 ns | 130.601 ± 1.169 ns | 403.928 ± 3.234 ns |
| massive/data-warehouse | 181.676 ± 1.122 µs | **141.822 ± 1.343 µs** | 890.366 ± 6.310 µs |
| massive/k8s-deployment | 182.018 ± 0.590 ns | **134.068 ± 0.658 ns** | 557.116 ± 4.116 ns |
| medium/user-profile | 376.545 ± 1.065 ns | **303.590 ± 2.601 ns** | 2.386 ± 0.017 µs |
| small/config | 293.721 ± 2.471 ns | **242.588 ± 0.970 ns** | 2.777 ± 0.019 µs |
| small/prettierrc | **533.369 ± 1.259 ns** | 920.456 ± 4.369 ns | 870.165 ± 5.689 ns |

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
| generated/deep-nesting | **11.202 ± 0.330 ms** | 108.435 ± 0.731 ms |
| generated/large-array | **1.798 ± 0.042 ms** | 87.040 ± 1.843 ms |
| generated/many-defs | **44.499 ± 0.432 ms** | 436.316 ± 2.313 ms |
| generated/wide-object | **145.031 ± 1.409 ms** | 507.047 ± 2.157 ms |
| large/api-catalog | **2.183 ± 0.038 ms** | 93.817 ± 0.744 ms |
| large/circleci | 4.222 ± 0.263 ms | — |
| massive/data-warehouse | **13.836 ± 0.127 ms** | 787.967 ± 3.480 ms |
| massive/k8s-deployment | **27.530 ± 0.255 ms** | 86.473 ± 2.716 ms |
| medium/user-profile | **1.859 ± 0.119 ms** | 87.770 ± 0.454 ms |
| small/config | **1.687 ± 0.037 ms** | 84.761 ± 0.637 ms |

### codegen — peak RSS

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | 45.39 MiB | **38.00 MiB** |
| generated/large-array | **3.81 MiB** | 33.31 ± 0.02 MiB |
| generated/many-defs | 115.12 ± 0.01 MiB | **97.95 ± 0.02 MiB** |
| generated/wide-object | **38.80 MiB** | 113.98 ± 0.11 MiB |
| large/api-catalog | **4.95 ± 0.01 MiB** | 34.75 MiB |
| large/circleci | 12.72 MiB | — |
| massive/data-warehouse | **36.95 ± 0.01 MiB** | 176.16 ± 0.06 MiB |
| massive/k8s-deployment | 95.78 ± 0.05 MiB | **31.78 ± 0.01 MiB** |
| medium/user-profile | **3.88 ± 0.01 MiB** | 33.55 ± 0.03 MiB |
| small/config | **3.34 MiB** | 32.94 ± 0.02 MiB |

### validate (valid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.378 ± 0.440 ms † | 4.080 ± 0.061 ms | **1.926 ± 0.037 ms** |
| generated/large-array | 58.829 ± 0.721 ms | **44.844 ± 0.525 ms** | 188.120 ± 1.734 ms |
| generated/many-defs | 23.125 ± 0.316 ms | **11.568 ± 0.380 ms** | 27.459 ± 1.528 ms |
| generated/wide-object | 27.913 ± 0.229 ms | 8.041 ± 0.058 ms | **4.052 ± 0.096 ms** |
| large/api-catalog | 3.145 ± 0.115 ms | 4.009 ± 0.059 ms | **1.584 ± 0.032 ms** |
| large/circleci | 6.900 ± 0.123 ms | 5.766 ± 0.095 ms | **2.050 ± 0.053 ms** |
| massive/data-warehouse | 50.329 ± 0.351 ms | 11.932 ± 0.116 ms | **4.298 ± 0.105 ms** |
| massive/k8s-deployment | 11.813 ± 0.084 ms | 317.860 ± 72.297 ms † | **2.614 ± 0.059 ms** |
| medium/user-profile | 2.898 ± 0.117 ms | 3.662 ± 0.083 ms | **1.517 ± 0.049 ms** |
| small/config | 2.887 ± 0.068 ms | 3.601 ± 0.341 ms | **1.488 ± 0.031 ms** |
| small/prettierrc | 3.057 ± 0.083 ms | 3.639 ± 0.054 ms | **1.537 ± 0.026 ms** |

### validate (valid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.91 ± 0.01 MiB | 9.89 ± 0.01 MiB | **3.97 MiB** |
| generated/large-array | **55.77 ± 0.01 MiB** | 64.67 MiB | 68.30 ± 0.01 MiB |
| generated/many-defs | 19.08 ± 0.01 MiB | 21.38 MiB | **14.41 ± 0.01 MiB** |
| generated/wide-object | 16.80 ± 0.44 MiB | 15.30 ± 0.02 MiB | **7.05 MiB** |
| large/api-catalog | 5.88 MiB | 10.36 ± 0.01 MiB | **3.08 MiB** |
| large/circleci | 9.14 ± 0.04 MiB | 13.06 ± 0.02 MiB | **4.27 ± 0.01 MiB** |
| massive/data-warehouse | 23.55 MiB | 20.97 MiB | **7.59 MiB** |
| massive/k8s-deployment | 11.78 ± 0.01 MiB | 20.45 ± 0.07 MiB | **6.23 ± 0.00 MiB** |
| medium/user-profile | 5.70 ± 0.00 MiB | 10.08 ± 0.01 MiB | **3.00 MiB** |
| small/config | 5.69 ± 0.01 MiB | 9.86 ± 0.00 MiB | **2.95 MiB** |
| small/prettierrc | 5.84 ± 0.02 MiB | 8.94 ± 0.01 MiB | **3.12 ± 0.01 MiB** |

### validate (invalid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.134 ± 0.061 ms | 4.054 ± 0.067 ms | **1.984 ± 0.071 ms** |
| generated/large-array | 58.974 ± 0.367 ms | **44.611 ± 0.548 ms** | 189.147 ± 2.402 ms |
| generated/many-defs | 23.071 ± 0.685 ms | **11.452 ± 0.608 ms** | 26.349 ± 0.452 ms |
| generated/wide-object | 27.956 ± 0.360 ms | 8.072 ± 0.094 ms | **4.289 ± 0.287 ms** |
| large/api-catalog | 3.095 ± 0.041 ms | 3.972 ± 0.160 ms | **1.597 ± 0.065 ms** |
| large/circleci | 6.877 ± 0.076 ms | 5.690 ± 0.137 ms | **1.976 ± 0.043 ms** |
| massive/data-warehouse | 51.392 ± 1.807 ms | 11.978 ± 0.220 ms | **4.471 ± 0.058 ms** |
| massive/k8s-deployment | 11.839 ± 0.190 ms | 255.339 ± 36.465 ms † | **2.640 ± 0.102 ms** |
| medium/user-profile | 2.859 ± 0.040 ms | 3.758 ± 0.079 ms | **1.510 ± 0.029 ms** |
| small/config | 2.829 ± 0.052 ms | 3.674 ± 0.731 ms † | **1.477 ± 0.035 ms** |
| small/prettierrc | 3.067 ± 0.069 ms | 3.768 ± 0.059 ms | **1.515 ± 0.042 ms** |

### validate (invalid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.89 ± 0.02 MiB | 10.03 ± 0.01 MiB | **4.03 MiB** |
| generated/large-array | **55.77 ± 0.01 MiB** | 64.84 MiB | 68.38 ± 0.02 MiB |
| generated/many-defs | 19.08 ± 0.42 MiB | 21.41 ± 0.02 MiB | **14.38 MiB** |
| generated/wide-object | 15.09 MiB | 15.39 ± 0.02 MiB | **7.30 ± 0.09 MiB** |
| large/api-catalog | 5.91 ± 0.02 MiB | 10.50 ± 0.01 MiB | **3.12 ± 0.01 MiB** |
| large/circleci | 9.12 ± 0.03 MiB | 13.12 ± 0.04 MiB | **4.27 MiB** |
| massive/data-warehouse | 24.42 MiB | 21.05 MiB | **7.39 ± 0.02 MiB** |
| massive/k8s-deployment | 11.77 ± 0.02 MiB | 20.50 ± 0.03 MiB | **6.25 MiB** |
| medium/user-profile | 5.72 ± 0.01 MiB | 10.23 MiB | **3.05 MiB** |
| small/config | 5.70 ± 0.01 MiB | 10.03 MiB | **3.02 MiB** |
| small/prettierrc | 5.88 MiB | 9.14 MiB | **3.14 MiB** |

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
