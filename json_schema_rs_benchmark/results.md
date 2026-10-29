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
| generated/deep-nesting | **6.417 ± 0.082 ms** | 9.663 ± 0.061 ms |
| generated/large-array | **74.931 ± 0.310 µs** | 1.031 ± 0.003 ms |
| generated/many-defs | **37.793 ± 0.150 ms** | 122.631 ± 1.768 ms |
| generated/wide-object | **138.250 ± 1.489 ms** | 148.003 ± 1.459 ms |
| large/api-catalog | **208.639 ± 0.207 µs** | 3.078 ± 0.003 ms |
| large/circleci | 1.438 ± 0.002 ms | — |
| massive/data-warehouse | **9.554 ± 0.008 ms** | 270.192 ± 0.654 ms |
| massive/k8s-deployment | 19.063 ± 0.170 ms | **1.811 ± 0.002 ms** |
| medium/user-profile | **93.516 ± 0.243 µs** | 1.409 ± 0.010 ms |
| small/config | **29.701 ± 0.027 µs** | 509.916 ± 1.403 µs |

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
| generated/deep-nesting | 1.024 ± 0.013 ms | 207.809 ± 0.361 µs | **63.425 ± 0.093 µs** |
| generated/large-array | 44.935 ± 0.080 µs | 20.868 ± 0.027 µs | **4.095 ± 0.007 µs** |
| generated/many-defs | 13.769 ± 0.168 ms | 3.009 ± 0.008 ms | **658.185 ± 11.037 µs** |
| generated/wide-object | 23.120 ± 0.035 ms | 2.524 ± 0.010 ms | **744.825 ± 0.755 µs** |
| large/api-catalog | 178.448 ± 0.137 µs | 104.114 ± 0.121 µs | **14.613 ± 0.019 µs** |
| large/circleci | 2.872 ± 0.013 ms | 1.021 ± 0.001 ms | **250.429 ± 0.305 µs** |
| massive/data-warehouse | 44.263 ± 0.429 ms | 6.320 ± 0.046 ms | **1.204 ± 0.002 ms** |
| massive/k8s-deployment | 7.377 ± 0.008 ms | 1.615 ± 0.002 ms | **590.581 ± 4.964 µs** |
| medium/user-profile | 67.005 ± 0.093 µs | 40.098 ± 0.064 µs | **5.376 ± 0.009 µs** |
| small/config | 26.973 ± 0.215 µs | 18.890 ± 0.038 µs | **1.774 ± 0.006 µs** |
| small/prettierrc | 205.014 ± 0.326 µs | 110.545 ± 0.212 µs | **24.014 ± 0.030 µs** |

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
| generated/deep-nesting | 6.353 ± 0.015 µs | **2.460 ± 0.016 µs** | 139.096 ± 0.223 µs |
| generated/large-array | 18.230 ± 0.093 ms | **7.456 ± 0.049 ms** | 148.330 ± 1.042 ms |
| generated/many-defs | 1.751 ± 0.005 ms | **617.592 ± 2.350 µs** | 19.280 ± 0.052 ms |
| generated/wide-object | 170.495 ± 0.233 µs | **112.209 ± 5.491 µs** | 795.442 ± 3.394 µs |
| large/api-catalog | 569.579 ± 1.562 ns | **239.873 ± 0.487 ns** | 6.715 ± 0.011 µs |
| large/circleci | 2.180 ± 0.038 µs | **464.790 ± 0.644 ns** | 2.201 ± 0.007 µs |
| massive/data-warehouse | 70.360 ± 0.080 µs | **23.640 ± 0.050 µs** | 627.984 ± 4.333 µs |
| massive/k8s-deployment | 521.649 ± 0.724 ns | **215.931 ± 0.767 ns** | 4.631 ± 0.011 µs |
| medium/user-profile | 854.704 ± 5.430 ns | **374.279 ± 1.732 ns** | 5.934 ± 0.020 µs |
| small/config | 225.406 ± 0.581 ns | **113.312 ± 0.816 ns** | 2.798 ± 0.015 µs |
| small/prettierrc | 731.389 ± 3.335 ns | **172.941 ± 0.871 ns** | 3.193 ± 0.007 µs |

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
| generated/deep-nesting | 7.394 ± 0.019 µs | **2.791 ± 0.004 µs** | 139.763 ± 0.196 µs |
| generated/large-array | 18.305 ± 0.035 ms | **7.483 ± 0.083 ms** | 148.424 ± 1.012 ms |
| generated/many-defs | 1.765 ± 0.005 ms | **620.409 ± 1.144 µs** | 18.901 ± 0.075 ms |
| generated/wide-object | 169.530 ± 0.310 µs | **110.556 ± 0.270 µs** | 796.441 ± 1.188 µs |
| large/api-catalog | 654.668 ± 2.741 ns | **393.673 ± 0.330 ns** | 5.408 ± 0.024 µs |
| large/circleci | **126.082 ± 0.213 ns** | 131.520 ± 0.250 ns | 428.909 ± 1.859 ns |
| massive/data-warehouse | 180.144 ± 0.404 µs | **136.025 ± 0.411 µs** | 916.441 ± 2.500 µs |
| massive/k8s-deployment | 176.331 ± 3.128 ns | **135.926 ± 0.631 ns** | 578.105 ± 3.053 ns |
| medium/user-profile | 370.246 ± 0.765 ns | **310.362 ± 0.586 ns** | 2.376 ± 0.006 µs |
| small/config | 287.560 ± 0.231 ns | **245.002 ± 0.595 ns** | 2.816 ± 0.008 µs |
| small/prettierrc | **537.273 ± 0.795 ns** | 898.807 ± 2.147 ns | 908.136 ± 5.823 ns |

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
| generated/deep-nesting | **11.327 ± 0.200 ms** | 108.725 ± 0.310 ms |
| generated/large-array | **1.909 ± 0.083 ms** | 87.642 ± 0.627 ms |
| generated/many-defs | **45.620 ± 0.168 ms** | 445.599 ± 1.981 ms |
| generated/wide-object | **143.756 ± 0.953 ms** | 515.223 ± 3.702 ms |
| large/api-catalog | **2.241 ± 0.048 ms** | 94.817 ± 0.458 ms |
| large/circleci | 4.282 ± 0.168 ms | — |
| massive/data-warehouse | **14.189 ± 0.209 ms** | 788.504 ± 4.202 ms |
| massive/k8s-deployment | **28.156 ± 0.373 ms** | 86.884 ± 0.835 ms |
| medium/user-profile | **1.927 ± 0.051 ms** | 88.900 ± 0.803 ms |
| small/config | **1.747 ± 0.052 ms** | 86.128 ± 0.540 ms |

### codegen — peak RSS

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | 45.50 MiB | **37.98 ± 0.02 MiB** |
| generated/large-array | **3.91 MiB** | 33.28 MiB |
| generated/many-defs | 116.00 MiB | **98.20 MiB** |
| generated/wide-object | **39.22 ± 0.01 MiB** | 115.27 ± 0.34 MiB |
| large/api-catalog | **4.98 MiB** | 34.66 ± 0.01 MiB |
| large/circleci | 13.25 MiB | — |
| massive/data-warehouse | **38.25 ± 0.07 MiB** | 176.52 ± 0.02 MiB |
| massive/k8s-deployment | 96.70 MiB | **31.81 ± 0.01 MiB** |
| medium/user-profile | **3.94 ± 0.01 MiB** | 33.58 MiB |
| small/config | **3.45 ± 0.01 MiB** | 32.91 ± 0.01 MiB |

### validate (valid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.386 ± 0.374 ms | 4.172 ± 0.284 ms | **1.949 ± 0.056 ms** |
| generated/large-array | 58.786 ± 0.168 ms | **45.022 ± 0.809 ms** | 189.627 ± 1.008 ms |
| generated/many-defs | 23.086 ± 0.289 ms | **11.493 ± 0.113 ms** | 26.293 ± 0.210 ms |
| generated/wide-object | 27.819 ± 0.208 ms | 8.115 ± 0.118 ms | **4.156 ± 0.041 ms** |
| large/api-catalog | 3.191 ± 0.058 ms | 4.037 ± 0.072 ms | **1.686 ± 0.048 ms** |
| large/circleci | 6.973 ± 0.074 ms | 5.803 ± 0.077 ms | **2.104 ± 0.038 ms** |
| massive/data-warehouse | 50.278 ± 1.418 ms | 11.948 ± 0.131 ms | **4.330 ± 0.120 ms** |
| massive/k8s-deployment | 11.962 ± 0.277 ms | 260.065 ± 34.728 ms † | **2.760 ± 0.052 ms** |
| medium/user-profile | 3.015 ± 0.068 ms | 3.782 ± 0.188 ms | **1.620 ± 0.041 ms** |
| small/config | 2.894 ± 0.064 ms | 3.750 ± 0.208 ms | **1.575 ± 0.034 ms** |
| small/prettierrc | 3.152 ± 0.160 ms | 3.779 ± 0.303 ms | **1.638 ± 0.030 ms** |

### validate (valid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.70 ± 0.03 MiB | 9.89 ± 0.02 MiB | **4.09 MiB** |
| generated/large-array | **55.78 MiB** | 64.69 MiB | 68.36 ± 0.01 MiB |
| generated/many-defs | 19.03 MiB | 21.36 ± 0.01 MiB | **14.44 ± 0.01 MiB** |
| generated/wide-object | 15.08 ± 0.01 MiB | 15.28 ± 0.00 MiB | **7.12 ± 0.01 MiB** |
| large/api-catalog | 5.84 MiB | 10.34 MiB | **3.23 ± 0.01 MiB** |
| large/circleci | 9.06 MiB | 13.03 MiB | **4.45 MiB** |
| massive/data-warehouse | 22.14 ± 0.57 MiB | 21.00 ± 0.02 MiB | **7.81 ± 0.01 MiB** |
| massive/k8s-deployment | 11.77 ± 0.02 MiB | 20.53 ± 0.03 MiB | **6.42 ± 0.01 MiB** |
| medium/user-profile | 5.70 MiB | 10.08 ± 0.02 MiB | **3.16 MiB** |
| small/config | 5.70 MiB | 9.88 MiB | **3.09 MiB** |
| small/prettierrc | 5.81 ± 0.02 MiB | 8.97 ± 0.01 MiB | **3.22 MiB** |

### validate (invalid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.243 ± 0.138 ms | 4.085 ± 0.046 ms | **2.002 ± 0.135 ms** |
| generated/large-array | 58.773 ± 0.254 ms | **44.665 ± 0.400 ms** | 189.538 ± 0.936 ms |
| generated/many-defs | 23.136 ± 0.249 ms | **11.584 ± 0.101 ms** | 26.317 ± 0.349 ms |
| generated/wide-object | 28.009 ± 0.312 ms | 8.239 ± 0.249 ms | **4.168 ± 0.086 ms** |
| large/api-catalog | 3.199 ± 0.037 ms | 4.023 ± 0.090 ms | **1.632 ± 0.025 ms** |
| large/circleci | 6.906 ± 0.072 ms | 6.025 ± 0.638 ms † | **2.065 ± 0.048 ms** |
| massive/data-warehouse | 51.112 ± 0.521 ms | 11.982 ± 0.086 ms | **4.644 ± 0.208 ms** |
| massive/k8s-deployment | 11.943 ± 0.045 ms | 291.016 ± 33.583 ms † | **3.105 ± 0.405 ms †** |
| medium/user-profile | 2.938 ± 0.053 ms | 3.871 ± 0.071 ms | **1.561 ± 0.037 ms** |
| small/config | 2.897 ± 0.095 ms | 3.701 ± 0.049 ms | **1.575 ± 0.043 ms** |
| small/prettierrc | 3.156 ± 0.058 ms | 3.759 ± 0.065 ms | **1.638 ± 0.074 ms** |

### validate (invalid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.70 MiB | 10.03 ± 0.01 MiB | **4.22 ± 0.01 MiB** |
| generated/large-array | **55.78 MiB** | 64.81 MiB | 68.45 ± 0.01 MiB |
| generated/many-defs | 19.05 ± 0.01 MiB | 21.44 ± 0.02 MiB | **14.73 MiB** |
| generated/wide-object | 15.06 ± 0.02 MiB | 15.41 MiB | **7.25 ± 0.01 MiB** |
| large/api-catalog | 5.88 MiB | 10.50 MiB | **3.27 MiB** |
| large/circleci | 9.00 ± 0.06 MiB | 13.28 ± 0.07 MiB | **4.47 MiB** |
| massive/data-warehouse | 22.75 MiB | 21.06 ± 0.01 MiB | **7.69 ± 0.02 MiB** |
| massive/k8s-deployment | 11.80 ± 0.01 MiB | 20.48 ± 0.01 MiB | **6.47 MiB** |
| medium/user-profile | 5.73 ± 0.03 MiB | 10.20 ± 0.01 MiB | **3.19 MiB** |
| small/config | 5.70 ± 0.02 MiB | 10.02 MiB | **3.19 MiB** |
| small/prettierrc | 5.83 ± 0.00 MiB | 9.12 ± 0.01 MiB | **3.23 ± 0.01 MiB** |

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
