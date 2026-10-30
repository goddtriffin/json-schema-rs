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
| generated/deep-nesting | **4.460 ± 0.088 ms** | 9.665 ± 0.115 ms |
| generated/large-array | **63.383 ± 0.435 µs** | 1.042 ± 0.003 ms |
| generated/many-defs | **26.896 ± 0.171 ms** | 122.372 ± 1.349 ms |
| generated/wide-object | **138.278 ± 0.391 ms** | 147.535 ± 0.156 ms |
| large/api-catalog | **195.033 ± 0.326 µs** | 3.103 ± 0.007 ms |
| large/circleci | 1.633 ± 0.002 ms | — |
| massive/data-warehouse | **7.795 ± 0.021 ms** | 270.049 ± 2.145 ms |
| massive/k8s-deployment | 14.863 ± 0.046 ms | **1.805 ± 0.004 ms** |
| medium/user-profile | **81.996 ± 0.096 µs** | 1.432 ± 0.022 ms |
| small/config | **27.096 ± 0.041 µs** | 517.080 ± 0.925 µs |

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
| generated/deep-nesting | 1.026 ± 0.002 ms | 207.807 ± 0.621 µs | **100.654 ± 0.738 µs** |
| generated/large-array | 44.785 ± 0.157 µs | 20.536 ± 0.079 µs | **10.492 ± 0.026 µs** |
| generated/many-defs | 13.665 ± 0.024 ms | 2.968 ± 0.003 ms | **1.606 ± 0.003 ms** |
| generated/wide-object | 23.172 ± 0.119 ms | 2.493 ± 0.012 ms | **1.297 ± 0.005 ms** |
| large/api-catalog | 177.443 ± 1.426 µs | 101.792 ± 0.150 µs | **42.371 ± 0.166 µs** |
| large/circleci | 2.867 ± 0.045 ms | 1.017 ± 0.004 ms | **509.379 ± 3.364 µs** |
| massive/data-warehouse | 43.987 ± 0.111 ms | 6.283 ± 0.083 ms | **2.573 ± 0.004 ms** |
| massive/k8s-deployment | 7.344 ± 0.010 ms | 1.587 ± 0.002 ms | **771.794 ± 3.504 µs** |
| medium/user-profile | 66.493 ± 0.077 µs | 39.620 ± 0.048 µs | **15.991 ± 0.036 µs** |
| small/config | 26.695 ± 0.075 µs | 18.892 ± 0.092 µs | **7.361 ± 0.029 µs** |
| small/prettierrc | 203.238 ± 0.276 µs | 111.187 ± 0.644 µs | **55.922 ± 0.059 µs** |

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
| generated/deep-nesting | 6.262 ± 0.071 µs | **2.436 ± 0.008 µs** | 3.037 ± 0.036 µs |
| generated/large-array | 17.993 ± 0.019 ms | **7.247 ± 0.016 ms** | 9.195 ± 0.071 ms |
| generated/many-defs | 1.721 ± 0.005 ms | **614.866 ± 6.391 µs** | 785.107 ± 2.851 µs |
| generated/wide-object | 170.469 ± 1.074 µs | 111.267 ± 2.418 µs | **55.977 ± 0.763 µs** |
| large/api-catalog | 560.112 ± 2.564 ns | **241.577 ± 1.220 ns** | 443.618 ± 2.255 ns |
| large/circleci | 2.195 ± 0.011 µs | **466.206 ± 1.766 ns** | 997.246 ± 13.478 ns |
| massive/data-warehouse | 68.982 ± 0.228 µs | **23.626 ± 0.076 µs** | 46.224 ± 0.159 µs |
| massive/k8s-deployment | 515.634 ± 0.819 ns | **209.447 ± 1.035 ns** | 485.830 ± 5.067 ns |
| medium/user-profile | 835.712 ± 1.471 ns | **369.041 ± 0.676 ns** | 498.004 ± 4.653 ns |
| small/config | 228.445 ± 0.580 ns | **112.287 ± 0.755 ns** | 141.490 ± 1.126 ns |
| small/prettierrc | 722.235 ± 2.734 ns | **172.999 ± 0.525 ns** | 276.725 ± 3.884 ns |

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
| generated/deep-nesting | 7.459 ± 0.010 µs | **2.768 ± 0.005 µs** | 4.160 ± 0.022 µs |
| generated/large-array | 17.986 ± 0.013 ms | **7.234 ± 0.029 ms** | 9.147 ± 0.065 ms |
| generated/many-defs | 1.730 ± 0.007 ms | **617.076 ± 2.022 µs** | 787.916 ± 8.616 µs |
| generated/wide-object | 167.094 ± 0.821 µs | 110.409 ± 0.378 µs | **56.623 ± 0.320 µs** |
| large/api-catalog | 661.687 ± 2.878 ns | **394.005 ± 0.968 ns** | 677.349 ± 1.166 ns |
| large/circleci | 126.249 ± 0.391 ns | 129.673 ± 0.618 ns | 205.184 ± 2.872 ns |
| massive/data-warehouse | 180.514 ± 0.849 µs | 137.837 ± 0.677 µs | **54.970 ± 0.185 µs** |
| massive/k8s-deployment | 172.413 ± 0.619 ns | **133.970 ± 0.477 ns** | 174.575 ± 0.539 ns |
| medium/user-profile | 364.984 ± 2.438 ns | **310.231 ± 1.089 ns** | 345.206 ± 1.171 ns |
| small/config | 283.215 ± 0.847 ns | **239.818 ± 0.711 ns** | 388.038 ± 3.452 ns |
| small/prettierrc | 524.944 ± 3.839 ns | 908.245 ± 1.840 ns | **106.609 ± 0.295 ns** |

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
| generated/deep-nesting | **8.485 ± 0.450 ms** | 107.855 ± 0.605 ms |
| generated/large-array | **1.839 ± 0.104 ms** | 86.840 ± 0.817 ms |
| generated/many-defs | **33.072 ± 0.592 ms** | 437.041 ± 1.053 ms |
| generated/wide-object | **138.604 ± 1.810 ms** | 509.888 ± 3.466 ms |
| large/api-catalog | **2.223 ± 0.059 ms** | 95.849 ± 1.441 ms |
| large/circleci | 4.586 ± 0.342 ms | — |
| massive/data-warehouse | **12.533 ± 0.159 ms** | 790.471 ± 3.771 ms |
| massive/k8s-deployment | **22.119 ± 0.317 ms** | 87.491 ± 1.695 ms |
| medium/user-profile | **1.851 ± 0.057 ms** | 88.060 ± 0.725 ms |
| small/config | **1.754 ± 0.045 ms** | 85.196 ± 0.821 ms |

### codegen — peak RSS

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | **24.31 MiB** | 37.98 MiB |
| generated/large-array | **3.95 ± 0.01 MiB** | 33.33 MiB |
| generated/many-defs | **58.66 MiB** | 98.11 MiB |
| generated/wide-object | **25.16 MiB** | 115.56 MiB |
| large/api-catalog | **4.86 ± 0.01 MiB** | 34.67 MiB |
| large/circleci | 10.98 ± 0.02 MiB | — |
| massive/data-warehouse | **32.83 MiB** | 176.80 ± 0.43 MiB |
| massive/k8s-deployment | 57.89 MiB | **31.73 ± 0.02 MiB** |
| medium/user-profile | **4.08 ± 0.01 MiB** | 33.58 MiB |
| small/config | **3.73 ± 0.01 MiB** | 32.92 MiB |

### validate (valid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.320 ± 0.665 ms † | 4.096 ± 0.226 ms | **1.741 ± 0.024 ms** |
| generated/large-array | 59.141 ± 0.642 ms | 45.218 ± 0.943 ms | 44.229 ± 0.398 ms |
| generated/many-defs | 23.378 ± 0.417 ms | 11.796 ± 0.504 ms | **7.031 ± 0.149 ms** |
| generated/wide-object | 28.158 ± 0.217 ms | 8.150 ± 0.066 ms | **3.778 ± 0.054 ms** |
| large/api-catalog | 3.180 ± 0.090 ms | 4.007 ± 0.059 ms | **1.846 ± 0.115 ms** |
| large/circleci | 6.899 ± 0.082 ms | 5.971 ± 0.573 ms | **2.511 ± 0.039 ms** |
| massive/data-warehouse | 49.962 ± 0.346 ms | 11.996 ± 0.241 ms | **5.219 ± 0.081 ms** |
| massive/k8s-deployment | 11.960 ± 0.268 ms | 314.933 ± 32.315 ms † | **2.831 ± 0.081 ms** |
| medium/user-profile | 2.914 ± 0.087 ms | 3.729 ± 0.211 ms | **1.600 ± 0.028 ms** |
| small/config | 2.877 ± 0.088 ms | 3.718 ± 0.094 ms | **1.568 ± 0.042 ms** |
| small/prettierrc | 3.078 ± 0.085 ms | 3.780 ± 0.110 ms | **1.758 ± 0.062 ms** |

### validate (valid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.83 MiB | 9.91 ± 0.02 MiB | **4.02 ± 0.01 MiB** |
| generated/large-array | **55.77 ± 0.01 MiB** | 64.69 MiB | 59.48 MiB |
| generated/many-defs | 19.09 MiB | 21.59 ± 0.08 MiB | **12.77 ± 0.06 MiB** |
| generated/wide-object | 16.75 ± 0.76 MiB | 15.28 ± 0.01 MiB | **7.19 MiB** |
| large/api-catalog | 5.88 MiB | 10.36 ± 0.01 MiB | **3.75 ± 0.02 MiB** |
| large/circleci | 9.09 ± 0.02 MiB | 13.03 ± 0.01 MiB | **5.30 ± 0.01 MiB** |
| massive/data-warehouse | 21.94 ± 0.01 MiB | 21.00 ± 0.02 MiB | **9.06 ± 0.02 MiB** |
| massive/k8s-deployment | 11.89 ± 0.02 MiB | 20.50 ± 0.03 MiB | **5.92 MiB** |
| medium/user-profile | 5.75 MiB | 10.09 ± 0.01 MiB | **3.62 MiB** |
| small/config | 5.77 MiB | 9.84 ± 0.03 MiB | **3.58 ± 0.00 MiB** |
| small/prettierrc | 5.83 ± 0.01 MiB | 8.94 ± 0.01 MiB | **3.75 MiB** |

### validate (invalid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.132 ± 0.100 ms | 4.030 ± 0.077 ms | **1.768 ± 0.079 ms** |
| generated/large-array | 59.153 ± 0.315 ms | 45.336 ± 6.893 ms † | 44.019 ± 0.329 ms |
| generated/many-defs | 23.167 ± 0.228 ms | 11.468 ± 0.095 ms | **6.939 ± 0.061 ms** |
| generated/wide-object | 27.850 ± 0.148 ms | 8.186 ± 0.288 ms | **3.849 ± 0.213 ms** |
| large/api-catalog | 3.134 ± 0.050 ms | 4.035 ± 0.080 ms | **1.756 ± 0.038 ms** |
| large/circleci | 7.121 ± 0.267 ms | 5.793 ± 0.133 ms | **2.490 ± 0.041 ms** |
| massive/data-warehouse | 51.051 ± 0.347 ms | 12.010 ± 0.077 ms | **5.140 ± 0.136 ms** |
| massive/k8s-deployment | 11.867 ± 0.076 ms | 326.112 ± 174.361 ms † | **2.939 ± 0.090 ms** |
| medium/user-profile | 2.970 ± 0.069 ms | 3.725 ± 0.159 ms | **1.580 ± 0.066 ms** |
| small/config | 2.833 ± 0.035 ms | 3.694 ± 0.097 ms | **1.540 ± 0.030 ms** |
| small/prettierrc | 3.086 ± 0.133 ms | 3.766 ± 0.065 ms | **1.698 ± 0.051 ms** |

### validate (invalid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.81 ± 0.02 MiB | 10.03 ± 0.01 MiB | **4.09 ± 0.02 MiB** |
| generated/large-array | **55.77 ± 0.01 MiB** | 64.81 ± 0.01 MiB | 59.53 ± 0.01 MiB |
| generated/many-defs | 19.09 MiB | 21.44 MiB | **12.86 ± 0.02 MiB** |
| generated/wide-object | 15.08 ± 0.02 MiB | 15.41 ± 0.01 MiB | **7.23 ± 0.05 MiB** |
| large/api-catalog | 5.83 ± 0.04 MiB | 10.50 ± 0.01 MiB | **3.77 ± 0.01 MiB** |
| large/circleci | 9.31 ± 0.31 MiB | 13.12 MiB | **5.30 ± 0.01 MiB** |
| massive/data-warehouse | 22.66 ± 0.01 MiB | 21.05 ± 0.02 MiB | **8.88 ± 0.08 MiB** |
| massive/k8s-deployment | 11.83 ± 0.02 MiB | 20.50 ± 0.03 MiB | **6.16 MiB** |
| medium/user-profile | 5.72 ± 0.00 MiB | 10.22 MiB | **3.64 ± 0.01 MiB** |
| small/config | 5.75 MiB | 10.02 MiB | **3.61 MiB** |
| small/prettierrc | 5.83 ± 0.01 MiB | 9.16 ± 0.01 MiB | **3.77 MiB** |

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
