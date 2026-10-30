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
| in-process | schema compile | peak heap | jsonschemars | 11 / 11 |
| in-process | validate (valid) | time | jsonschema | 10 / 11 |
| in-process | validate (valid) | peak heap | jsonschemars | 10 / 11 |
| in-process | validate (invalid) | time | jsonschema | 7 / 11 |
| in-process | validate (invalid) | peak heap | jsonschema | 7 / 11 |
| CLI | codegen | time | jsonschemars | 9 / 9 |
| CLI | codegen | peak RSS | jsonschemars | 8 / 9 |
| CLI | validate (valid) | time | jsonschemars | 10 / 11 |
| CLI | validate (valid) | peak RSS | jsonschemars | 10 / 11 |
| CLI | validate (invalid) | time | jsonschemars | 10 / 11 |
| CLI | validate (invalid) | peak RSS | jsonschemars | 10 / 11 |

## In-process (library calls)

### codegen — time

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | **4.451 ± 0.048 ms** | 9.718 ± 0.031 ms |
| generated/large-array | **62.799 ± 0.162 µs** | 1.032 ± 0.001 ms |
| generated/many-defs | **26.898 ± 0.052 ms** | 123.193 ± 0.201 ms |
| generated/wide-object | **136.229 ± 0.588 ms** | 148.150 ± 1.064 ms |
| large/api-catalog | **195.400 ± 0.666 µs** | 3.111 ± 0.031 ms |
| large/circleci | 1.618 ± 0.002 ms | — |
| massive/data-warehouse | **7.878 ± 0.014 ms** | 269.794 ± 5.668 ms |
| massive/k8s-deployment | 15.224 ± 0.038 ms | **1.804 ± 0.001 ms** |
| medium/user-profile | **81.840 ± 0.800 µs** | 1.424 ± 0.006 ms |
| small/config | **27.250 ± 0.020 µs** | 511.718 ± 0.552 µs |

### codegen — peak heap

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | 19.03 MiB | **12.65 MiB** |
| generated/large-array | **250.76 KiB** | 1.23 MiB |
| generated/many-defs | **41.42 MiB** | 133.38 MiB |
| generated/wide-object | **19.36 MiB** | 162.36 MiB |
| large/api-catalog | **667.22 KiB** | 3.64 MiB |
| large/circleci | 5.90 MiB | — |
| massive/data-warehouse | **24.57 MiB** | 288.90 MiB |
| massive/k8s-deployment | 47.59 MiB | **2.79 MiB** |
| medium/user-profile | **248.33 KiB** | 1.71 MiB |
| small/config | **95.91 KiB** | 639.58 KiB |

### schema compile — time

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 1.025 ± 0.019 ms | 205.889 ± 0.498 µs | **101.078 ± 0.078 µs** |
| generated/large-array | 44.845 ± 0.062 µs | 20.567 ± 0.045 µs | **10.621 ± 0.027 µs** |
| generated/many-defs | 13.667 ± 0.058 ms | 2.993 ± 0.026 ms | **1.604 ± 0.002 ms** |
| generated/wide-object | 23.239 ± 0.031 ms | 2.507 ± 0.004 ms | **1.300 ± 0.002 ms** |
| large/api-catalog | 179.493 ± 0.354 µs | 102.346 ± 0.233 µs | **42.195 ± 0.057 µs** |
| large/circleci | 2.865 ± 0.014 ms | 1.010 ± 0.005 ms | **511.787 ± 0.636 µs** |
| massive/data-warehouse | 44.415 ± 0.054 ms | 6.280 ± 0.016 ms | **2.565 ± 0.007 ms** |
| massive/k8s-deployment | 7.407 ± 0.027 ms | 1.578 ± 0.003 ms | **741.933 ± 1.237 µs** |
| medium/user-profile | 67.023 ± 0.063 µs | 39.833 ± 0.087 µs | **15.917 ± 0.023 µs** |
| small/config | 26.939 ± 0.138 µs | 18.857 ± 0.136 µs | **7.363 ± 0.021 µs** |
| small/prettierrc | 203.612 ± 0.467 µs | 110.129 ± 0.124 µs | **56.180 ± 0.065 µs** |

### schema compile — peak heap

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 636.04 KiB | 534.41 KiB | **152.81 KiB** |
| generated/large-array | 66.67 KiB | 34.10 KiB | **20.41 KiB** |
| generated/many-defs | 8.10 MiB | 3.88 MiB | **2.64 MiB** |
| generated/wide-object | 13.73 MiB | 4.03 MiB | **2.69 MiB** |
| large/api-catalog | 266.71 KiB | 151.24 KiB | **67.54 KiB** |
| large/circleci | 3.85 MiB | 1.75 MiB | **869.03 KiB** |
| massive/data-warehouse | 16.73 MiB | 9.42 MiB | **4.01 MiB** |
| massive/k8s-deployment | 7.45 MiB | 3.28 MiB | **1.45 MiB** |
| medium/user-profile | 121.82 KiB | 59.13 KiB | **28.40 KiB** |
| small/config | 38.35 KiB | 24.08 KiB | **10.83 KiB** |
| small/prettierrc | 476.85 KiB | 261.04 KiB | **115.30 KiB** |

### validate (valid) — time

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.735 ± 0.021 µs | **2.423 ± 0.005 µs** | 3.130 ± 0.011 µs |
| generated/large-array | 17.860 ± 0.103 ms | **7.239 ± 0.027 ms** | 9.265 ± 0.046 ms |
| generated/many-defs | 1.740 ± 0.026 ms | **622.826 ± 1.058 µs** | 797.112 ± 2.341 µs |
| generated/wide-object | 170.322 ± 0.453 µs | 107.107 ± 0.450 µs | **56.046 ± 0.337 µs** |
| large/api-catalog | 560.252 ± 1.900 ns | **241.031 ± 0.796 ns** | 438.857 ± 1.374 ns |
| large/circleci | 2.178 ± 0.009 µs | **466.758 ± 0.636 ns** | 1.003 ± 0.008 µs |
| massive/data-warehouse | 69.360 ± 0.114 µs | **23.766 ± 0.059 µs** | 46.672 ± 0.270 µs |
| massive/k8s-deployment | 515.551 ± 1.571 ns | **209.542 ± 0.336 ns** | 490.659 ± 2.231 ns |
| medium/user-profile | 834.685 ± 0.976 ns | **371.970 ± 5.165 ns** | 500.400 ± 4.381 ns |
| small/config | 223.245 ± 0.477 ns | **112.289 ± 0.293 ns** | 135.311 ± 0.858 ns |
| small/prettierrc | 725.169 ± 4.156 ns | **171.473 ± 1.203 ns** | 283.399 ± 2.349 ns |

### validate (valid) — peak heap

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 2.25 KiB | 0.03 KiB | **0.00 KiB** |
| generated/large-array | 0.19 KiB | 0.03 KiB | **0.00 KiB** |
| generated/many-defs | 0.56 KiB | 0.03 KiB | **0.00 KiB** |
| generated/wide-object | 0.19 KiB | 0.03 KiB | **0.00 KiB** |
| large/api-catalog | 0.19 KiB | 0.03 KiB | **0.00 KiB** |
| large/circleci | 2.58 KiB | **0.03 KiB** | 0.22 KiB |
| massive/data-warehouse | 0.19 KiB | 0.03 KiB | **0.00 KiB** |
| massive/k8s-deployment | 0.19 KiB | 0.03 KiB | **0.00 KiB** |
| medium/user-profile | 0.19 KiB | 0.03 KiB | **0.00 KiB** |
| small/config | 0.19 KiB | 0.03 KiB | **0.00 KiB** |
| small/prettierrc | 0.77 KiB | 0.03 KiB | **0.00 KiB** |

### validate (invalid) — time

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 7.550 ± 0.018 µs | **2.776 ± 0.005 µs** | 4.276 ± 0.073 µs |
| generated/large-array | 17.917 ± 0.036 ms | **7.258 ± 0.121 ms** | 9.130 ± 0.251 ms |
| generated/many-defs | 1.752 ± 0.011 ms | **627.608 ± 0.946 µs** | 771.727 ± 3.177 µs |
| generated/wide-object | 170.365 ± 0.750 µs | 107.507 ± 0.291 µs | **56.138 ± 0.418 µs** |
| large/api-catalog | 647.111 ± 3.746 ns | **392.969 ± 1.059 ns** | 700.414 ± 2.287 ns |
| large/circleci | 125.797 ± 0.263 ns | 127.377 ± 0.242 ns | 210.763 ± 1.106 ns |
| massive/data-warehouse | 181.242 ± 0.235 µs | 137.769 ± 0.423 µs | **55.331 ± 0.111 µs** |
| massive/k8s-deployment | 171.542 ± 0.225 ns | **134.139 ± 1.102 ns** | 181.453 ± 1.037 ns |
| medium/user-profile | 361.675 ± 1.101 ns | **308.129 ± 0.496 ns** | 356.530 ± 1.646 ns |
| small/config | 281.851 ± 0.333 ns | **243.356 ± 3.790 ns** | 394.256 ± 0.881 ns |
| small/prettierrc | 539.780 ± 1.114 ns | 892.564 ± 3.962 ns | **108.590 ± 0.299 ns** |

### validate (invalid) — peak heap

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 2.97 KiB | **0.75 KiB** | 2.02 KiB |
| generated/large-array | 1.37 KiB | **0.46 KiB** | 0.52 KiB |
| generated/many-defs | 7.77 KiB | **0.77 KiB** | 0.82 KiB |
| generated/wide-object | 0.74 KiB | **0.41 KiB** | 0.45 KiB |
| large/api-catalog | 1.44 KiB | **0.83 KiB** | 1.64 KiB |
| large/circleci | 0.74 KiB | **0.40 KiB** | 0.87 KiB |
| massive/data-warehouse | 467.89 KiB | 111.78 KiB | **0.47 KiB** |
| massive/k8s-deployment | 0.81 KiB | 0.48 KiB | 0.47 KiB |
| medium/user-profile | 1.30 KiB | 0.73 KiB | **0.63 KiB** |
| small/config | 1.30 KiB | **0.71 KiB** | 1.35 KiB |
| small/prettierrc | 2.38 KiB | 2.69 KiB | **0.41 KiB** |

## CLI (end to end)

### codegen — time

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | **8.410 ± 0.532 ms** | 107.354 ± 0.298 ms |
| generated/large-array | **1.790 ± 0.023 ms** | 86.112 ± 0.875 ms |
| generated/many-defs | **32.541 ± 0.638 ms** | 435.643 ± 1.161 ms |
| generated/wide-object | **137.728 ± 3.715 ms** | 507.779 ± 3.708 ms |
| large/api-catalog | **2.113 ± 0.046 ms** | 94.863 ± 1.673 ms |
| large/circleci | 4.282 ± 0.065 ms | — |
| massive/data-warehouse | **12.476 ± 0.390 ms** | 787.304 ± 3.020 ms |
| massive/k8s-deployment | **21.993 ± 0.183 ms** | 86.063 ± 1.178 ms |
| medium/user-profile | **1.847 ± 0.039 ms** | 86.719 ± 0.851 ms |
| small/config | **1.787 ± 0.112 ms** | 83.630 ± 0.681 ms |

### codegen — peak RSS

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | **24.30 ± 0.02 MiB** | 38.02 MiB |
| generated/large-array | **3.92 MiB** | 33.31 MiB |
| generated/many-defs | **57.25 ± 0.51 MiB** | 98.09 MiB |
| generated/wide-object | **25.05 ± 0.01 MiB** | 115.33 ± 0.58 MiB |
| large/api-catalog | **4.80 MiB** | 34.62 ± 0.01 MiB |
| large/circleci | 10.86 ± 0.01 MiB | — |
| massive/data-warehouse | **32.88 ± 0.01 MiB** | 177.12 ± 0.71 MiB |
| massive/k8s-deployment | 57.69 ± 0.02 MiB | **31.80 ± 0.01 MiB** |
| medium/user-profile | **4.00 ± 0.00 MiB** | 33.55 ± 0.01 MiB |
| small/config | **3.73 MiB** | 32.95 MiB |

### validate (valid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.308 ± 0.417 ms | 3.991 ± 0.187 ms | **1.731 ± 0.154 ms** |
| generated/large-array | 58.876 ± 1.037 ms | 45.226 ± 0.857 ms | 44.693 ± 0.527 ms |
| generated/many-defs | 22.960 ± 0.281 ms | 11.969 ± 1.086 ms | **7.035 ± 0.109 ms** |
| generated/wide-object | 27.809 ± 0.863 ms | 8.043 ± 0.090 ms | **3.734 ± 0.051 ms** |
| large/api-catalog | 3.050 ± 0.045 ms | 3.928 ± 0.181 ms | **1.645 ± 0.028 ms** |
| large/circleci | 6.766 ± 0.229 ms | 5.649 ± 0.106 ms | **2.437 ± 0.051 ms** |
| massive/data-warehouse | 49.754 ± 0.288 ms | 11.810 ± 0.076 ms | **5.067 ± 0.077 ms** |
| massive/k8s-deployment | 11.899 ± 0.226 ms | 397.241 ± 101.950 ms † | **2.815 ± 0.056 ms** |
| medium/user-profile | 2.834 ± 0.129 ms | 3.726 ± 0.130 ms | **1.588 ± 0.037 ms** |
| small/config | 2.812 ± 0.091 ms | 3.600 ± 0.142 ms | **1.581 ± 0.050 ms** |
| small/prettierrc | 3.008 ± 0.254 ms | 3.705 ± 0.102 ms | **1.669 ± 0.051 ms** |

### validate (valid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.83 ± 0.01 MiB | 9.88 ± 0.01 MiB | **3.95 ± 0.01 MiB** |
| generated/large-array | **55.80 ± 0.01 MiB** | 64.70 ± 0.01 MiB | 59.41 MiB |
| generated/many-defs | 19.11 MiB | 21.66 ± 0.16 MiB | **12.86 MiB** |
| generated/wide-object | 15.11 ± 0.01 MiB | 15.28 ± 0.01 MiB | **7.12 ± 0.01 MiB** |
| large/api-catalog | 5.89 ± 0.02 MiB | 10.36 MiB | **3.66 MiB** |
| large/circleci | 9.09 ± 0.01 MiB | 13.09 MiB | **5.20 ± 0.01 MiB** |
| massive/data-warehouse | 21.94 ± 0.01 MiB | 20.97 ± 0.02 MiB | **9.12 MiB** |
| massive/k8s-deployment | 11.86 ± 0.01 MiB | 20.58 MiB | **5.83 ± 0.02 MiB** |
| medium/user-profile | 5.75 ± 0.01 MiB | 10.06 MiB | **3.50 ± 0.01 MiB** |
| small/config | 5.77 ± 0.01 MiB | 9.86 ± 0.01 MiB | **3.47 MiB** |
| small/prettierrc | 5.83 ± 0.01 MiB | 8.94 MiB | **3.66 MiB** |

### validate (invalid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.043 ± 0.127 ms | 3.991 ± 0.060 ms | **1.787 ± 0.106 ms** |
| generated/large-array | 58.905 ± 0.464 ms | 44.743 ± 6.900 ms † | 45.112 ± 0.452 ms |
| generated/many-defs | 23.091 ± 0.406 ms | 11.419 ± 0.121 ms | **6.965 ± 0.100 ms** |
| generated/wide-object | 27.810 ± 0.227 ms | 7.993 ± 0.092 ms | **3.737 ± 0.178 ms** |
| large/api-catalog | 3.073 ± 0.225 ms | 3.863 ± 0.389 ms † | **1.692 ± 0.076 ms** |
| large/circleci | 6.752 ± 0.081 ms | 5.584 ± 0.070 ms | **2.461 ± 0.047 ms** |
| massive/data-warehouse | 50.933 ± 0.459 ms | 11.804 ± 0.095 ms | **5.021 ± 0.124 ms** |
| massive/k8s-deployment | 11.668 ± 0.121 ms | 378.072 ± 63.426 ms † | **2.814 ± 0.078 ms** |
| medium/user-profile | 2.829 ± 0.148 ms | 3.650 ± 0.071 ms | **1.601 ± 0.036 ms** |
| small/config | 2.777 ± 0.139 ms | 3.607 ± 0.062 ms | **1.548 ± 0.027 ms** |
| small/prettierrc | 3.008 ± 0.037 ms | 3.617 ± 0.070 ms | **1.640 ± 0.028 ms** |

### validate (invalid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.81 ± 0.02 MiB | 10.02 ± 0.02 MiB | **4.03 MiB** |
| generated/large-array | **55.80 MiB** | 64.81 MiB | 59.47 MiB |
| generated/many-defs | 20.75 MiB | 21.42 ± 0.01 MiB | **12.91 ± 0.01 MiB** |
| generated/wide-object | 15.11 ± 0.01 MiB | 15.39 ± 0.01 MiB | **7.20 ± 0.01 MiB** |
| large/api-catalog | 5.88 ± 0.07 MiB | 10.50 MiB | **3.70 MiB** |
| large/circleci | 9.12 ± 0.01 MiB | 13.11 MiB | **5.23 ± 0.01 MiB** |
| massive/data-warehouse | 22.62 MiB | 21.05 MiB | **9.00 MiB** |
| massive/k8s-deployment | 11.81 ± 0.01 MiB | 20.55 ± 0.04 MiB | **5.83 ± 0.01 MiB** |
| medium/user-profile | 5.73 ± 0.01 MiB | 10.22 MiB | **3.56 ± 0.01 MiB** |
| small/config | 5.73 MiB | 10.02 MiB | **3.53 MiB** |
| small/prettierrc | 5.83 MiB | 9.16 MiB | **3.70 MiB** |

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
