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
| in-process | codegen | peak heap | jsonschemars | 8 / 9 |
| in-process | schema compile | time | jsonschemars | 11 / 11 |
| in-process | schema compile | peak heap | jsonschemars | 11 / 11 |
| in-process | validate (valid) | time | jsonschema | 10 / 11 |
| in-process | validate (valid) | peak heap | jsonschemars | 10 / 11 |
| in-process | validate (invalid) | time | jsonschema | 7 / 11 |
| in-process | validate (invalid) | peak heap | jsonschema | 7 / 11 |
| CLI | codegen | time | jsonschemars | 9 / 9 |
| CLI | codegen | peak RSS | jsonschemars | 9 / 9 |
| CLI | validate (valid) | time | jsonschemars | 10 / 11 |
| CLI | validate (valid) | peak RSS | jsonschemars | 10 / 11 |
| CLI | validate (invalid) | time | jsonschemars | 10 / 11 |
| CLI | validate (invalid) | peak RSS | jsonschemars | 10 / 11 |

## In-process (library calls)

### codegen — time

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | **382.203 ± 0.371 µs** | 9.806 ± 0.079 ms |
| generated/large-array | **45.983 ± 0.069 µs** | 1.055 ± 0.001 ms |
| generated/many-defs | **4.448 ± 0.010 ms** | 124.362 ± 0.255 ms |
| generated/wide-object | **5.747 ± 0.015 ms** | 149.786 ± 0.300 ms |
| large/api-catalog | **167.626 ± 2.135 µs** | 3.178 ± 0.016 ms |
| large/circleci | 1.723 ± 0.006 ms | — |
| massive/data-warehouse | **8.696 ± 0.013 ms** | 274.188 ± 1.998 ms |
| massive/k8s-deployment | 4.767 ± 0.011 ms | **1.853 ± 0.003 ms** |
| medium/user-profile | **62.020 ± 0.141 µs** | 1.456 ± 0.004 ms |
| small/config | **20.956 ± 0.048 µs** | 520.082 ± 0.639 µs |
| small/prettierrc | 330.963 ± 4.652 µs | — |

### codegen — peak heap

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | **342.37 KiB** | 12.65 MiB |
| generated/large-array | **30.47 KiB** | 1.23 MiB |
| generated/many-defs | **3.28 MiB** | 133.38 MiB |
| generated/wide-object | **4.66 MiB** | 162.36 MiB |
| large/api-catalog | **95.84 KiB** | 3.64 MiB |
| large/circleci | 1.55 MiB | — |
| massive/data-warehouse | **5.65 MiB** | 288.90 MiB |
| massive/k8s-deployment | 3.42 MiB | **2.79 MiB** |
| medium/user-profile | **41.04 KiB** | 1.71 MiB |
| small/config | **15.46 KiB** | 639.58 KiB |
| small/prettierrc | 189.57 KiB | — |

### schema compile — time

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 1.043 ± 0.005 ms | 210.392 ± 5.332 µs | **102.262 ± 0.493 µs** |
| generated/large-array | 45.429 ± 0.066 µs | 20.945 ± 0.065 µs | **10.782 ± 0.038 µs** |
| generated/many-defs | 13.979 ± 0.063 ms | 3.058 ± 0.006 ms | **1.634 ± 0.001 ms** |
| generated/wide-object | 23.582 ± 0.031 ms | 2.556 ± 0.004 ms | **1.333 ± 0.004 ms** |
| large/api-catalog | 180.580 ± 0.319 µs | 104.418 ± 0.384 µs | **43.123 ± 0.084 µs** |
| large/circleci | 2.910 ± 0.016 ms | 1.030 ± 0.003 ms | **519.343 ± 1.167 µs** |
| massive/data-warehouse | 45.073 ± 0.078 ms | 6.441 ± 0.010 ms | **2.626 ± 0.003 ms** |
| massive/k8s-deployment | 7.507 ± 0.017 ms | 1.623 ± 0.004 ms | **754.103 ± 8.853 µs** |
| medium/user-profile | 67.952 ± 0.109 µs | 40.342 ± 0.142 µs | **16.373 ± 0.086 µs** |
| small/config | 27.327 ± 0.069 µs | 19.257 ± 0.029 µs | **7.492 ± 0.019 µs** |
| small/prettierrc | 207.534 ± 0.472 µs | 112.677 ± 0.082 µs | **57.607 ± 0.092 µs** |

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
| generated/deep-nesting | 6.454 ± 0.011 µs | **2.469 ± 0.005 µs** | 3.190 ± 0.012 µs |
| generated/large-array | 18.447 ± 0.109 ms | **7.365 ± 0.013 ms** | 9.229 ± 0.130 ms |
| generated/many-defs | 1.741 ± 0.010 ms | **631.224 ± 4.665 µs** | 790.512 ± 4.779 µs |
| generated/wide-object | 174.172 ± 0.893 µs | 116.183 ± 1.660 µs | **55.599 ± 0.716 µs** |
| large/api-catalog | 572.684 ± 4.292 ns | **243.320 ± 0.753 ns** | 450.559 ± 2.247 ns |
| large/circleci | 2.236 ± 0.007 µs | **475.257 ± 2.358 ns** | 1.017 ± 0.025 µs |
| massive/data-warehouse | 71.602 ± 0.558 µs | **24.150 ± 0.033 µs** | 47.740 ± 0.394 µs |
| massive/k8s-deployment | 526.923 ± 0.702 ns | **212.474 ± 1.071 ns** | 494.029 ± 1.284 ns |
| medium/user-profile | 858.218 ± 2.614 ns | **381.340 ± 10.918 ns** | 510.700 ± 3.191 ns |
| small/config | 229.537 ± 0.963 ns | **114.432 ± 0.407 ns** | 140.294 ± 2.273 ns |
| small/prettierrc | 734.315 ± 6.971 ns | **175.673 ± 1.197 ns** | 281.647 ± 4.707 ns |

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
| generated/deep-nesting | 7.677 ± 0.010 µs | **2.831 ± 0.005 µs** | 4.228 ± 0.043 µs |
| generated/large-array | 18.394 ± 0.042 ms | **7.377 ± 0.041 ms** | 9.233 ± 0.140 ms |
| generated/many-defs | 1.739 ± 0.017 ms | **628.852 ± 1.584 µs** | 808.286 ± 4.148 µs |
| generated/wide-object | 172.681 ± 0.957 µs | 114.559 ± 1.983 µs | **55.773 ± 0.304 µs** |
| large/api-catalog | 682.903 ± 1.900 ns | **400.959 ± 1.005 ns** | 693.976 ± 4.369 ns |
| large/circleci | **131.673 ± 0.265 ns** | 135.992 ± 1.779 ns | 208.687 ± 4.571 ns |
| massive/data-warehouse | 186.319 ± 1.565 µs | 139.804 ± 0.856 µs | **55.800 ± 0.270 µs** |
| massive/k8s-deployment | 184.398 ± 0.204 ns | **136.393 ± 0.747 ns** | 177.995 ± 0.946 ns |
| medium/user-profile | 378.535 ± 0.901 ns | **313.949 ± 1.380 ns** | 362.823 ± 4.544 ns |
| small/config | 294.488 ± 0.750 ns | **248.747 ± 0.740 ns** | 398.885 ± 0.992 ns |
| small/prettierrc | 547.077 ± 2.284 ns | 915.520 ± 2.909 ns | **110.460 ± 0.614 ns** |

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
| generated/deep-nesting | **2.495 ± 0.068 ms** | 111.485 ± 1.182 ms |
| generated/large-array | **1.849 ± 0.063 ms** | 88.103 ± 2.123 ms |
| generated/many-defs | **7.402 ± 0.155 ms** | 436.863 ± 2.588 ms |
| generated/wide-object | **9.022 ± 0.199 ms** | 506.118 ± 3.489 ms |
| large/api-catalog | **2.115 ± 0.041 ms** | 95.817 ± 1.060 ms |
| large/circleci | 4.386 ± 0.242 ms | — |
| massive/data-warehouse | **12.339 ± 0.126 ms** | 786.538 ± 3.598 ms |
| massive/k8s-deployment | **8.085 ± 0.074 ms** | 86.028 ± 0.658 ms |
| medium/user-profile | **1.871 ± 0.049 ms** | 91.125 ± 4.819 ms |
| small/config | **1.828 ± 0.225 ms †** | 85.825 ± 1.360 ms |
| small/prettierrc | 2.426 ± 0.044 ms | — |

### codegen — peak RSS

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | **4.92 ± 0.01 MiB** | 38.03 ± 0.02 MiB |
| generated/large-array | **3.62 MiB** | 33.31 ± 0.02 MiB |
| generated/many-defs | **8.58 ± 0.01 MiB** | 98.05 MiB |
| generated/wide-object | **11.39 ± 0.09 MiB** | 115.12 ± 0.03 MiB |
| large/api-catalog | **4.19 ± 0.02 MiB** | 34.70 MiB |
| large/circleci | 6.61 ± 0.04 MiB | — |
| massive/data-warehouse | **11.39 ± 0.01 MiB** | 176.88 ± 0.12 MiB |
| massive/k8s-deployment | **8.91 ± 0.05 MiB** | 31.78 ± 0.01 MiB |
| medium/user-profile | **3.70 MiB** | 33.58 ± 0.01 MiB |
| small/config | **3.53 MiB** | 32.97 ± 0.02 MiB |
| small/prettierrc | 4.59 ± 0.01 MiB | — |

### validate (valid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.533 ± 0.430 ms | 4.517 ± 0.177 ms | **1.955 ± 0.093 ms** |
| generated/large-array | 60.601 ± 2.815 ms | 44.913 ± 1.310 ms | 46.061 ± 0.514 ms |
| generated/many-defs | 23.033 ± 0.268 ms | 11.821 ± 0.320 ms | **7.152 ± 0.620 ms** |
| generated/wide-object | 27.694 ± 0.101 ms | 8.250 ± 0.508 ms | **3.840 ± 0.083 ms** |
| large/api-catalog | 3.202 ± 0.069 ms | 4.017 ± 0.056 ms | **1.804 ± 0.062 ms** |
| large/circleci | 6.875 ± 0.107 ms | 5.913 ± 0.407 ms | **2.567 ± 0.020 ms** |
| massive/data-warehouse | 49.654 ± 0.277 ms | 11.998 ± 0.244 ms | **5.239 ± 0.088 ms** |
| massive/k8s-deployment | 11.891 ± 0.137 ms | 361.351 ± 115.781 ms † | **2.917 ± 0.061 ms** |
| medium/user-profile | 2.983 ± 0.053 ms | 3.803 ± 0.150 ms | **1.666 ± 0.053 ms** |
| small/config | 2.924 ± 0.064 ms | 3.766 ± 0.055 ms | **1.694 ± 0.054 ms** |
| small/prettierrc | 3.172 ± 0.161 ms | 3.785 ± 0.315 ms | **1.731 ± 0.053 ms** |

### validate (valid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.84 ± 0.01 MiB | 9.91 ± 0.01 MiB | **3.91 MiB** |
| generated/large-array | **55.83 ± 0.02 MiB** | 64.69 ± 0.03 MiB | 59.38 ± 0.02 MiB |
| generated/many-defs | 19.11 MiB | 21.36 MiB | **12.80 ± 0.15 MiB** |
| generated/wide-object | 15.11 ± 0.01 MiB | 15.27 ± 0.02 MiB | **7.16 ± 0.00 MiB** |
| large/api-catalog | 5.89 MiB | 10.36 MiB | **3.69 MiB** |
| large/circleci | 9.14 ± 0.02 MiB | 13.11 ± 0.04 MiB | **5.23 ± 0.01 MiB** |
| massive/data-warehouse | 23.75 MiB | 21.48 MiB | **9.17 MiB** |
| massive/k8s-deployment | 11.81 ± 0.01 MiB | 20.48 ± 0.03 MiB | **5.88 ± 0.02 MiB** |
| medium/user-profile | 5.73 ± 0.02 MiB | 10.09 ± 0.01 MiB | **3.53 MiB** |
| small/config | 5.75 MiB | 9.86 MiB | **3.48 ± 0.01 MiB** |
| small/prettierrc | 5.83 ± 0.02 MiB | 8.97 ± 0.02 MiB | **3.66 ± 0.01 MiB** |

### validate (invalid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.433 ± 0.151 ms | 4.187 ± 0.073 ms | **1.909 ± 0.035 ms** |
| generated/large-array | 59.006 ± 0.938 ms | **44.834 ± 0.633 ms** | 46.425 ± 0.764 ms |
| generated/many-defs | 23.025 ± 0.312 ms | 11.449 ± 0.424 ms | **7.160 ± 0.349 ms** |
| generated/wide-object | 27.938 ± 0.600 ms | 8.078 ± 0.086 ms | **3.829 ± 0.338 ms** |
| large/api-catalog | 3.188 ± 0.031 ms | 3.991 ± 0.078 ms | **1.759 ± 0.032 ms** |
| large/circleci | 6.970 ± 0.214 ms | 5.741 ± 0.076 ms | **2.538 ± 0.025 ms** |
| massive/data-warehouse | 50.910 ± 0.487 ms | 11.936 ± 0.247 ms | **5.143 ± 0.224 ms** |
| massive/k8s-deployment | 11.886 ± 0.051 ms | 299.888 ± 44.728 ms † | **2.873 ± 0.057 ms** |
| medium/user-profile | 3.001 ± 0.146 ms | 3.819 ± 0.308 ms | **1.673 ± 0.042 ms** |
| small/config | 2.936 ± 0.176 ms | 3.778 ± 0.073 ms | **1.622 ± 0.053 ms** |
| small/prettierrc | 3.226 ± 0.161 ms | 3.783 ± 0.065 ms | **1.736 ± 0.064 ms** |

### validate (invalid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.83 MiB | 10.02 ± 0.01 MiB | **4.00 ± 0.00 MiB** |
| generated/large-array | **55.81 ± 0.02 MiB** | 64.80 MiB | 59.45 MiB |
| generated/many-defs | 19.11 MiB | 21.41 ± 0.01 MiB | **12.92 ± 0.04 MiB** |
| generated/wide-object | 15.11 MiB | 15.42 MiB | **7.22 MiB** |
| large/api-catalog | 5.84 ± 0.01 MiB | 10.50 MiB | **3.70 MiB** |
| large/circleci | 9.11 ± 0.01 MiB | 13.09 ± 0.02 MiB | **5.23 MiB** |
| massive/data-warehouse | 22.62 ± 0.01 MiB | 21.09 ± 0.04 MiB | **9.38 ± 0.23 MiB** |
| massive/k8s-deployment | 11.84 MiB | 20.45 MiB | **5.89 ± 0.02 MiB** |
| medium/user-profile | 5.70 MiB | 10.20 ± 0.00 MiB | **3.55 MiB** |
| small/config | 5.75 ± 0.01 MiB | 9.98 ± 0.02 MiB | **3.55 MiB** |
| small/prettierrc | 5.83 ± 0.00 MiB | 9.12 ± 0.01 MiB | **3.67 MiB** |

## Compatibility

205 of 228 attempted cells pass. Rows with at least one failure:

| Tier | Operation | Fixture | boon | jsonschema | jsonschema-cli | jsonschemars | typify |
|---|---|---|---|---|---|---|---|
| in-process | codegen | large/circleci | — | — | — | pass | FAIL: panic: not implemented: if/then/else schemas are not supported: { "if": { "required": [ "type" ], "properties": { "type": { "const": "release" } } }, "then": { "required": [ "plan_name" ], "properties": { "plan_name": { "type": "string", "markdownDescription": "Required plan name for release jobs." } }, "additionalProperties": true, "markdownDescription": "https://circleci.com/docs/reference/configuration-reference/#job-type\n\nRelease job." }, "else": { "if": { "required": [ "type" ], "properties": { "type": { "enum": [ "lock", "unlock" ] } } }, "then": { "required": [ "key" ], "properties": { "key": { "type": "string", "markdownDescription": "Required key for lock/unlock jobs." } }, "additionalProperties": true, "markdownDescription": "https://circleci.com/docs/reference/configuration-reference/#job-type\n\nA lock/unlock job" }, "else": { "if": { "required": [ "type" ], "properties": { "type": { "enum": [ "approval", "no-op" ] } } }, "then": { "additionalProperties": true, "markdownDescription": "https://circleci.com/docs/reference/configuration-reference/#job-type\n\nApproval/no-op jobs primarily act as workflow markers. Other keys like `steps:` are permitted but will be ignored in the final pipeline." }, "else": { "if": { "anyOf": [ { "not": { "required": [ "type" ] } }, { "required": [ "type" ], "properties": { "type": { "const": "build" } } } ], "markdownDescription": "https://circleci.com/docs/reference/configuration-reference/#job-type\n\nA build job. The job explicitly has type 'build' or defaults to build when no type specified." }, "then": { "anyOf": [ { "type": "object", "required": [ "executor" ], "markdownDescription": "A job that uses an executor defined elsewhere in the configuration." }, { "oneOf": [ { "type": "object", "required": [ "machine" ] }, { "type": "object", "required": [ "docker" ] }, { "type": "object", "required": [ "macos" ] } ] } ], "required": [ "steps" ], "properties": { "circleci_ip_ranges": { "type": "boolean", "markdownDescription": "Enable running jobs with a set of well-defined IP address ranges." }, "description": { "type": "string" }, "docker": { "type": "array", "items": { "type": "object", "required": [ "image" ], "properties": { "auth": { "type": "object", "required": [ "password", "username" ], "properties": { "password": { "type": "string" }, "username": { "type": "string" } }, "additionalProperties": false }, "aws_auth": { "oneOf": [ { "type": "object", "required": [ "aws_access_key_id", "aws_secret_access_key" ], "properties": { "aws_access_key_id": { "type": "string" }, "aws_secret_access_key": { "type": "string" } }, "additionalProperties": false }, { "type": "object", "required": [ "oidc_role_arn" ], "properties": { "oidc_role_arn": { "type": "string" } }, "additionalProperties": false } ] }, "command": { "oneOf": [ { "type": "string" }, { "type": "array", "items": { "type": "string" } } ] }, "entrypoint": { "oneOf": [ { "type": "string" }, { "type": "array", "items": { "type": "string" } } ] }, "environment": { "oneOf": [ { "type": [ "string", "null" ], "markdownDescription": "A map of environment variable names and values." }, { "type": "array", "items": { "$ref": "#/definitions/environment" }, "markdownDescription": "An array of strings in the form KEY=VALUE" }, { "type": "object", "additionalProperties": { "type": [ "string", "boolean", "number", "null" ] }, "markdownDescription": "The value should be a string, but we allow null, boolean and numbers too. Examples: `FOO: true` and `BAR: 2` `BAZ:`" } ] }, "image": { "type": "string" }, "name": { "type": "string" }, "user": { "type": "string" } }, "additionalProperties": false }, "minItems": 1 }, "environment": { "oneOf": [ { "type": [ "string", "null" ], "markdownDescription": "A map of environment variable names and values." }, { "type": "array", "items": { "$ref": "#/definitions/environment" }, "markdownDescription": "An array of strings in the form KEY=VALUE" }, { "type": "object", "additionalProperties": { "type": [ "string", "boolean", "number", "null" ] }, "markdownDescription": "The value should be a string, but we allow null, boolean and numbers too. Examples: `FOO: true` and `BAR: 2` `BAZ:`" } ] }, "executor": { "oneOf": [ { "type": "string", "markdownDescription": "https://circleci.com/docs/reference/reusing-config/#executor\n\nThe name of the executor to use (defined via the top level executors map or from an orb)." }, { "type": "object", "minProperties": 1, "required": [ "name" ], "properties": { "name": { "type": "string" } }, "additionalProperties": true, "markdownDescription": "Executor stanza to use for the job, with optional parameter overrides." } ] }, "machine": { "oneOf": [ { "anyOf": [ { "type": "string", "pattern": " *<< *parameters.([^ ]+) *>> *" }, { "type": "boolean" }, { "type": "number" }, { "type": "string" } ] }, { "type": "object", "properties": { "docker_layer_caching": { "anyOf": [ { "type": "string", "pattern": " *<< *parameters.([^ ]+) *>> *" }, { "type": "boolean" }, { "type": "number" }, { "type": "string" } ], "markdownDescription": "Set to `true` to enable [Docker Layer Caching](https://circleci.com/docs/docker-layer-caching). Note: If you haven't already, you must open a support ticket to have a CircleCI Sales representative contact you about enabling this feature on your account for an additional fee." }, "enabled": { "anyOf": [ { "type": "string", "pattern": " *<< *parameters.([^ ]+) *>> *" }, { "type": "boolean" }, { "type": "number" }, { "type": "string" } ], "markdownDescription": "A boolean or a template parameter evaluating to a boolean" }, "image": { "type": "string" }, "resource_class": { "type": "string" }, "shell": { "type": "string" } }, "additionalProperties": false } ], "markdownDescription": "Use the default machine executor image" }, "macos": { "type": "object", "required": [ "xcode" ], "properties": { "resource_class": { "type": "string" }, "shell": { "type": "string" }, "xcode": { "type": [ "string", "number" ] } }, "additionalProperties": false }, "parallelism": { "anyOf": [ { "oneOf": [ { "type": "string", "pattern": " *<< *parameters.([^ ]+) *>> *" }, { "type": "string", "pattern": " *<< *pipeline.parameters.([^ ]+) *>> *" } ] }, { "type": "integer" } ], "markdownDescription": "A integer or a parameter evaluating to a integer" }, "parameters": { "type": "object", "minProperties": 1, "additionalProperties": { "type": "object", "required": [ "type" ], "properties": { "default": { "oneOf": [ { "type": "string" }, { "type": "boolean" }, { "type": "integer" }, { "type": "array", "items": { "$ref": "#/definitions/step" } } ] }, "description": { "type": "string", "markdownDescription": "A description of the parameter" }, "enum": { "type": "array", "items": { "type": "string" }, "minItems": 1, "markdownDescription": "List of allowed values for an `enum` type parameter" }, "type": { "type": "string", "enum": [ "boolean", "string", "steps", "enum", "executor", "integer", "env_var_name" ], "markdownDescription": "The parameter type. See [Parameter Types](https://circleci.com/docs/reusing-config#parameter-syntax) for details:\n- `boolean`: A boolean parameter\n- `string`: A string parameter \n- `integer`: An integer parameter\n- `enum`: An enum parameter (list of allowed string values)\n- `executor`: An executor parameter (allows job to choose executor)\n- `steps`: A steps parameter (allows mixing predefined and user-defined steps)\n- `env_var_name`: An environment variable name parameter (must match POSIX_NAME regexp)" } }, "additionalProperties": false }, "markdownDescription": "https://circleci.com/docs/reusing-config#using-the-parameters-declaration\n\nA map of parameter keys." }, "resource_class": { "type": "string" }, "retention": { "type": "object", "properties": { "caches": { "type": "string", "pattern": "^([1-9]\|1[0-5])d$" } }, "additionalProperties": false }, "shell": { "oneOf": [ { "type": "string" }, { "type": "array", "items": { "type": "string" } } ] }, "steps": { "type": "array", "items": { "$ref": "#/definitions/step" }, "minItems": 1 }, "type": { "const": "build", "markdownDescription": "The job type. If not specified, defaults to build." }, "working_directory": { "type": "string", "markdownDescription": "In which directory to run the steps. (default: `~/project`. `project` is a literal string, not the name of the project.) You can also refer the directory with `$CIRCLE_WORKING_DIRECTORY` environment variable." } }, "additionalProperties": false, "markdownDescription": "Validation: The job must adhere to the structure of a build job." }, "else": false } } } } |
| in-process | codegen | medium/package-json | — | — | — | FAIL: not measured: schema needs external documents (offline run) | FAIL: not measured: schema needs external documents (offline run) |
| in-process | codegen | small/prettierrc | — | — | — | pass | FAIL: codegen error: value does not conform to the given schema |
| in-process | schema compile | medium/package-json | FAIL: not measured: schema needs external documents (offline run) | FAIL: not measured: schema needs external documents (offline run) | — | FAIL: not measured: schema needs external documents (offline run) | — |
| in-process | validate (valid) | medium/package-json | FAIL: not measured: schema needs external documents (offline run) | FAIL: not measured: schema needs external documents (offline run) | — | FAIL: not measured: schema needs external documents (offline run) | — |
| in-process | validate (invalid) | medium/package-json | FAIL: not measured: schema needs external documents (offline run) | FAIL: not measured: schema needs external documents (offline run) | — | FAIL: not measured: schema needs external documents (offline run) | — |
| CLI | codegen | large/circleci | — | — | — | pass | FAIL: exit 101: not implemented: if/then/else schemas are not supported: { |
| CLI | codegen | medium/package-json | — | — | — | FAIL: not measured: schema needs external documents (offline run) | FAIL: not measured: schema needs external documents (offline run) |
| CLI | codegen | small/prettierrc | — | — | — | pass | FAIL: exit 1: value does not conform to the given schema |
| CLI | validate (valid) | medium/package-json | FAIL: not measured: schema needs external documents (offline run) | — | FAIL: not measured: schema needs external documents (offline run) | FAIL: not measured: schema needs external documents (offline run) | — |
| CLI | validate (invalid) | medium/package-json | FAIL: not measured: schema needs external documents (offline run) | — | FAIL: not measured: schema needs external documents (offline run) | FAIL: not measured: schema needs external documents (offline run) | — |
