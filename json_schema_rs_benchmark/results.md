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
| in-process | validate (invalid) | time | jsonschema | 10 / 11 |
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
| generated/deep-nesting | **6.450 ± 0.019 ms** | 9.673 ± 0.083 ms |
| generated/large-array | **75.779 ± 0.190 µs** | 1.035 ± 0.001 ms |
| generated/many-defs | **38.237 ± 0.179 ms** | 122.372 ± 1.977 ms |
| generated/wide-object | **140.012 ± 0.727 ms** | 147.572 ± 0.738 ms |
| large/api-catalog | **211.021 ± 0.375 µs** | 3.111 ± 0.035 ms |
| large/circleci | 1.451 ± 0.003 ms | — |
| massive/data-warehouse | **9.819 ± 0.024 ms** | 269.875 ± 0.669 ms |
| massive/k8s-deployment | 19.251 ± 0.400 ms | **1.827 ± 0.005 ms** |
| medium/user-profile | **94.804 ± 0.043 µs** | 1.431 ± 0.015 ms |
| small/config | **30.187 ± 0.270 µs** | 521.636 ± 8.627 µs |

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
| generated/deep-nesting | 1.026 ± 0.013 ms | 207.665 ± 0.971 µs | **66.195 ± 0.146 µs** |
| generated/large-array | 45.004 ± 0.071 µs | 21.005 ± 0.041 µs | **4.284 ± 0.006 µs** |
| generated/many-defs | 13.607 ± 0.039 ms | 3.020 ± 0.004 ms | **683.938 ± 11.764 µs** |
| generated/wide-object | 23.247 ± 0.044 ms | 2.545 ± 0.008 ms | **789.405 ± 1.814 µs** |
| large/api-catalog | 178.391 ± 0.152 µs | 103.456 ± 0.068 µs | **15.443 ± 0.030 µs** |
| large/circleci | 2.867 ± 0.013 ms | 1.020 ± 0.001 ms | **257.843 ± 0.794 µs** |
| massive/data-warehouse | 44.669 ± 0.188 ms | 6.366 ± 0.011 ms | **1.251 ± 0.006 ms** |
| massive/k8s-deployment | 7.312 ± 0.012 ms | 1.614 ± 0.006 ms | **618.036 ± 1.997 µs** |
| medium/user-profile | 67.040 ± 0.140 µs | 40.536 ± 0.055 µs | **5.640 ± 0.013 µs** |
| small/config | 27.024 ± 0.161 µs | 19.153 ± 0.097 µs | **1.855 ± 0.007 µs** |
| small/prettierrc | 206.030 ± 0.378 µs | 112.995 ± 0.135 µs | **25.339 ± 0.033 µs** |

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
| generated/deep-nesting | 6.392 ± 0.019 µs | **2.441 ± 0.006 µs** | 141.232 ± 0.149 µs |
| generated/large-array | 18.014 ± 0.113 ms | **7.334 ± 0.063 ms** | 149.407 ± 0.738 ms |
| generated/many-defs | 1.756 ± 0.006 ms | **615.040 ± 11.342 µs** | 19.208 ± 0.031 ms |
| generated/wide-object | 170.803 ± 0.970 µs | **108.673 ± 2.367 µs** | 804.895 ± 1.681 µs |
| large/api-catalog | 559.223 ± 1.295 ns | **241.211 ± 2.126 ns** | 6.883 ± 0.021 µs |
| large/circleci | 2.233 ± 0.014 µs | **464.993 ± 1.905 ns** | 2.238 ± 0.009 µs |
| massive/data-warehouse | 68.733 ± 0.157 µs | **23.642 ± 0.053 µs** | 632.550 ± 2.504 µs |
| massive/k8s-deployment | 515.437 ± 1.606 ns | **214.931 ± 0.741 ns** | 4.680 ± 0.015 µs |
| medium/user-profile | 845.699 ± 6.432 ns | **377.981 ± 3.819 ns** | 5.977 ± 0.019 µs |
| small/config | 225.017 ± 0.187 ns | **113.718 ± 0.197 ns** | 2.848 ± 0.013 µs |
| small/prettierrc | 722.697 ± 3.740 ns | **178.435 ± 2.452 ns** | 3.192 ± 0.008 µs |

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
| generated/deep-nesting | 7.505 ± 0.012 µs | **2.796 ± 0.011 µs** | 141.867 ± 0.134 µs |
| generated/large-array | 18.020 ± 0.049 ms | **7.361 ± 0.036 ms** | 151.070 ± 1.406 ms |
| generated/many-defs | 1.753 ± 0.007 ms | **622.911 ± 1.767 µs** | 19.079 ± 0.364 ms |
| generated/wide-object | 170.808 ± 0.218 µs | **109.142 ± 0.365 µs** | 807.422 ± 2.370 µs |
| large/api-catalog | 690.478 ± 2.201 ns | **395.239 ± 1.497 ns** | 5.424 ± 0.013 µs |
| large/circleci | 138.550 ± 0.322 ns | **131.336 ± 0.904 ns** | 431.820 ± 1.782 ns |
| massive/data-warehouse | 180.093 ± 0.993 µs | **141.920 ± 0.278 µs** | 921.013 ± 3.260 µs |
| massive/k8s-deployment | 205.406 ± 0.545 ns | **135.174 ± 0.727 ns** | 585.226 ± 2.073 ns |
| medium/user-profile | 373.861 ± 0.998 ns | **309.846 ± 0.688 ns** | 2.437 ± 0.012 µs |
| small/config | 290.300 ± 0.597 ns | **246.392 ± 1.021 ns** | 2.882 ± 0.011 µs |
| small/prettierrc | **538.386 ± 1.193 ns** | 903.990 ± 3.655 ns | 908.611 ± 1.405 ns |

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
| generated/deep-nesting | **11.187 ± 0.252 ms** | 108.441 ± 1.423 ms |
| generated/large-array | **1.820 ± 0.050 ms** | 88.248 ± 1.592 ms |
| generated/many-defs | **45.175 ± 0.207 ms** | 440.574 ± 3.827 ms |
| generated/wide-object | **142.555 ± 2.097 ms** | 511.449 ± 4.467 ms |
| large/api-catalog | **2.176 ± 0.036 ms** | 94.336 ± 0.388 ms |
| large/circleci | 4.142 ± 0.058 ms | — |
| massive/data-warehouse | **14.144 ± 0.169 ms** | 791.741 ± 4.434 ms |
| massive/k8s-deployment | **28.284 ± 0.630 ms** | 86.890 ± 1.187 ms |
| medium/user-profile | **1.876 ± 0.043 ms** | 88.508 ± 0.668 ms |
| small/config | **1.704 ± 0.041 ms** | 85.356 ± 0.764 ms |

### codegen — peak RSS

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | 45.50 MiB | **38.00 MiB** |
| generated/large-array | **3.92 MiB** | 33.31 MiB |
| generated/many-defs | 116.05 MiB | **98.67 ± 0.34 MiB** |
| generated/wide-object | **40.02 ± 0.28 MiB** | 115.22 ± 0.75 MiB |
| large/api-catalog | **5.03 MiB** | 34.67 MiB |
| large/circleci | 13.33 MiB | — |
| massive/data-warehouse | **38.27 ± 0.01 MiB** | 176.55 ± 0.17 MiB |
| massive/k8s-deployment | 96.97 MiB | **31.80 ± 0.01 MiB** |
| medium/user-profile | **3.95 ± 0.01 MiB** | 33.58 ± 0.00 MiB |
| small/config | **3.45 MiB** | 32.94 ± 0.02 MiB |

### validate (valid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.330 ± 0.352 ms | 4.199 ± 0.534 ms † | **1.966 ± 0.041 ms** |
| generated/large-array | 58.927 ± 0.440 ms | **44.837 ± 0.901 ms** | 189.137 ± 1.941 ms |
| generated/many-defs | 23.042 ± 0.243 ms | **11.475 ± 0.134 ms** | 26.325 ± 0.214 ms |
| generated/wide-object | 27.858 ± 0.130 ms | 8.293 ± 0.480 ms | **4.237 ± 0.158 ms** |
| large/api-catalog | 3.130 ± 0.085 ms | 4.034 ± 0.088 ms | **1.639 ± 0.079 ms** |
| large/circleci | 6.897 ± 0.208 ms | 5.736 ± 0.099 ms | **2.039 ± 0.038 ms** |
| massive/data-warehouse | 50.118 ± 0.511 ms | 11.984 ± 0.136 ms | **4.590 ± 0.349 ms** |
| massive/k8s-deployment | 11.898 ± 0.207 ms | 342.183 ± 329.429 ms † | **2.711 ± 0.047 ms** |
| medium/user-profile | 2.901 ± 0.070 ms | 3.773 ± 0.086 ms | **1.594 ± 0.058 ms** |
| small/config | 2.804 ± 0.032 ms | 3.602 ± 0.265 ms | **1.513 ± 0.040 ms** |
| small/prettierrc | 3.089 ± 0.092 ms | 3.702 ± 0.187 ms | **1.560 ± 0.025 ms** |

### validate (valid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.89 MiB | 9.86 ± 0.02 MiB | **4.20 ± 0.02 MiB** |
| generated/large-array | **55.80 ± 0.03 MiB** | 64.69 MiB | 68.38 ± 0.04 MiB |
| generated/many-defs | 19.06 ± 0.01 MiB | 21.34 ± 0.02 MiB | **14.36 ± 0.01 MiB** |
| generated/wide-object | 15.14 MiB | 15.30 ± 0.01 MiB | **7.16 MiB** |
| large/api-catalog | 5.86 ± 0.00 MiB | 10.36 ± 0.01 MiB | **3.33 MiB** |
| large/circleci | 9.19 ± 0.01 MiB | 13.06 ± 0.01 MiB | **4.48 MiB** |
| massive/data-warehouse | 23.05 MiB | 20.98 ± 0.02 MiB | **7.89 ± 0.02 MiB** |
| massive/k8s-deployment | 11.86 MiB | 20.52 MiB | **6.44 ± 0.01 MiB** |
| medium/user-profile | 5.69 ± 0.01 MiB | 10.08 MiB | **3.20 MiB** |
| small/config | 5.67 ± 0.01 MiB | 9.86 ± 0.01 MiB | **3.14 MiB** |
| small/prettierrc | 5.80 ± 0.01 MiB | 8.94 ± 0.01 MiB | **3.28 MiB** |

### validate (invalid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.133 ± 0.117 ms | 4.151 ± 0.054 ms | **1.986 ± 0.124 ms** |
| generated/large-array | 58.852 ± 0.318 ms | **44.852 ± 0.591 ms** | 188.528 ± 1.260 ms |
| generated/many-defs | 23.154 ± 0.235 ms | **11.627 ± 0.415 ms** | 26.408 ± 0.217 ms |
| generated/wide-object | 27.889 ± 0.205 ms | 8.216 ± 0.310 ms | **4.214 ± 0.206 ms** |
| large/api-catalog | 3.113 ± 0.119 ms | 3.997 ± 0.116 ms | **1.565 ± 0.033 ms** |
| large/circleci | 6.802 ± 0.044 ms | 5.708 ± 0.053 ms | **2.008 ± 0.031 ms** |
| massive/data-warehouse | 51.169 ± 1.563 ms | 11.938 ± 0.070 ms | **4.680 ± 0.170 ms** |
| massive/k8s-deployment | 11.786 ± 0.107 ms | 311.848 ± 393.027 ms † | **2.733 ± 0.072 ms** |
| medium/user-profile | 2.877 ± 0.115 ms | 3.732 ± 0.074 ms | **1.512 ± 0.029 ms** |
| small/config | 2.852 ± 0.052 ms | 3.563 ± 0.054 ms | **1.493 ± 0.035 ms** |
| small/prettierrc | 3.128 ± 0.066 ms | 3.760 ± 0.142 ms | **1.523 ± 0.029 ms** |

### validate (invalid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.89 ± 0.00 MiB | 10.02 ± 0.01 MiB | **4.27 MiB** |
| generated/large-array | **55.77 ± 0.01 MiB** | 64.83 MiB | 68.39 ± 0.01 MiB |
| generated/many-defs | 20.31 MiB | 21.41 ± 0.01 MiB | **14.47 MiB** |
| generated/wide-object | 15.20 MiB | 15.42 MiB | **7.20 ± 0.14 MiB** |
| large/api-catalog | 5.86 ± 0.02 MiB | 10.50 ± 0.01 MiB | **3.34 ± 0.01 MiB** |
| large/circleci | 9.16 ± 0.02 MiB | 13.12 ± 0.01 MiB | **4.42 ± 0.00 MiB** |
| massive/data-warehouse | 22.69 ± 0.06 MiB | 21.06 MiB | **7.70 MiB** |
| massive/k8s-deployment | 11.83 MiB | 20.53 ± 0.02 MiB | **6.42 ± 0.01 MiB** |
| medium/user-profile | 5.70 ± 0.01 MiB | 10.20 MiB | **3.25 MiB** |
| small/config | 5.66 ± 0.01 MiB | 10.00 ± 0.00 MiB | **3.22 MiB** |
| small/prettierrc | 5.81 ± 0.00 MiB | 9.12 ± 0.01 MiB | **3.27 MiB** |

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
