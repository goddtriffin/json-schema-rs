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
| in-process | validate (valid) | time | jsonschema | 11 / 11 |
| in-process | validate (valid) | peak heap | jsonschema | 11 / 11 |
| in-process | validate (invalid) | time | jsonschema | 9 / 11 |
| in-process | validate (invalid) | peak heap | jsonschema | 9 / 11 |
| CLI | codegen | time | jsonschemars | 9 / 9 |
| CLI | codegen | peak RSS | jsonschemars | 8 / 9 |
| CLI | validate (valid) | time | jsonschemars | 9 / 11 |
| CLI | validate (valid) | peak RSS | jsonschemars | 10 / 11 |
| CLI | validate (invalid) | time | jsonschemars | 9 / 11 |
| CLI | validate (invalid) | peak RSS | jsonschemars | 10 / 11 |

## In-process (library calls)

### codegen — time

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | **4.653 ± 0.168 ms** | 9.732 ± 0.379 ms |
| generated/large-array | **57.637 ± 0.593 µs** | 1.041 ± 0.020 ms |
| generated/many-defs | **27.891 ± 1.089 ms** | 123.079 ± 3.129 ms |
| generated/wide-object | **137.856 ± 0.973 ms** | 147.056 ± 1.643 ms |
| large/api-catalog | **184.459 ± 2.028 µs** | 3.102 ± 0.002 ms |
| large/circleci | 1.425 ± 0.032 ms | — |
| massive/data-warehouse | **6.753 ± 0.026 ms** | 269.325 ± 3.402 ms |
| massive/k8s-deployment | 14.805 ± 0.046 ms | **1.811 ± 0.003 ms** |
| medium/user-profile | **71.028 ± 0.080 µs** | 1.418 ± 0.002 ms |
| small/config | **23.486 ± 0.056 µs** | 509.251 ± 0.994 µs |

### codegen — peak heap

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | 19.02 MiB | **12.65 MiB** |
| generated/large-array | **247.00 KiB** | 1.23 MiB |
| generated/many-defs | **41.29 MiB** | 133.38 MiB |
| generated/wide-object | **19.36 MiB** | 162.36 MiB |
| large/api-catalog | **663.92 KiB** | 3.64 MiB |
| large/circleci | 5.90 MiB | — |
| massive/data-warehouse | **24.47 MiB** | 288.90 MiB |
| massive/k8s-deployment | 47.59 MiB | **2.79 MiB** |
| medium/user-profile | **243.84 KiB** | 1.71 MiB |
| small/config | **92.52 KiB** | 639.58 KiB |

### schema compile — time

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 1.023 ± 0.018 ms | 205.224 ± 0.874 µs | **58.100 ± 0.077 µs** |
| generated/large-array | 44.422 ± 0.024 µs | 20.639 ± 0.032 µs | **3.374 ± 0.011 µs** |
| generated/many-defs | 13.678 ± 0.015 ms | 3.002 ± 0.009 ms | **553.810 ± 0.609 µs** |
| generated/wide-object | 23.200 ± 0.046 ms | 2.503 ± 0.006 ms | **615.897 ± 4.364 µs** |
| large/api-catalog | 178.169 ± 0.322 µs | 102.663 ± 0.073 µs | **12.303 ± 0.032 µs** |
| large/circleci | 2.866 ± 0.013 ms | 1.015 ± 0.003 ms | **205.316 ± 0.164 µs** |
| massive/data-warehouse | 44.020 ± 0.231 ms | 6.294 ± 0.021 ms | **1.017 ± 0.002 ms** |
| massive/k8s-deployment | 7.345 ± 0.031 ms | 1.576 ± 0.002 ms | **391.414 ± 0.238 µs** |
| medium/user-profile | 66.762 ± 0.059 µs | 40.107 ± 0.041 µs | **4.444 ± 0.014 µs** |
| small/config | 27.158 ± 0.076 µs | 19.132 ± 0.057 µs | **1.528 ± 0.006 µs** |
| small/prettierrc | 204.818 ± 0.236 µs | 109.782 ± 0.087 µs | **21.374 ± 0.072 µs** |

### schema compile — peak heap

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 636.04 KiB | 534.41 KiB | **106.91 KiB** |
| generated/large-array | 66.67 KiB | 34.10 KiB | **13.51 KiB** |
| generated/many-defs | 8.10 MiB | 3.88 MiB | **1.96 MiB** |
| generated/wide-object | 13.73 MiB | 4.03 MiB | **1.83 MiB** |
| large/api-catalog | 266.71 KiB | 151.24 KiB | **44.75 KiB** |
| large/circleci | 3.85 MiB | 1.75 MiB | **661.11 KiB** |
| massive/data-warehouse | 16.73 MiB | 9.42 MiB | **3.03 MiB** |
| massive/k8s-deployment | 7.45 MiB | 3.28 MiB | **1.07 MiB** |
| medium/user-profile | 121.82 KiB | 59.13 KiB | **17.09 KiB** |
| small/config | 38.35 KiB | 24.08 KiB | **5.16 KiB** |
| small/prettierrc | 476.85 KiB | 261.04 KiB | **82.88 KiB** |

### validate (valid) — time

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.776 ± 0.023 µs | **2.450 ± 0.030 µs** | 137.485 ± 0.190 µs |
| generated/large-array | 17.837 ± 0.047 ms | **7.261 ± 0.054 ms** | 143.779 ± 0.831 ms |
| generated/many-defs | 1.793 ± 0.050 ms | **624.488 ± 1.707 µs** | 18.680 ± 0.055 ms |
| generated/wide-object | 170.926 ± 0.731 µs | **115.666 ± 0.630 µs** | 660.378 ± 3.000 µs |
| large/api-catalog | 559.504 ± 3.452 ns | **252.133 ± 0.696 ns** | 6.710 ± 0.019 µs |
| large/circleci | 2.199 ± 0.011 µs | **471.628 ± 7.883 ns** | 2.117 ± 0.011 µs |
| massive/data-warehouse | 70.222 ± 0.208 µs | **23.631 ± 0.042 µs** | 628.531 ± 1.164 µs |
| massive/k8s-deployment | 515.955 ± 1.044 ns | **214.533 ± 1.092 ns** | 4.650 ± 0.009 µs |
| medium/user-profile | 840.193 ± 2.110 ns | **370.566 ± 2.067 ns** | 5.931 ± 0.019 µs |
| small/config | 223.380 ± 0.415 ns | **112.577 ± 0.328 ns** | 2.793 ± 0.055 µs |
| small/prettierrc | 720.511 ± 6.726 ns | **173.616 ± 1.544 ns** | 2.972 ± 0.008 µs |

### validate (valid) — peak heap

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 2.25 KiB | **0.03 KiB** | 11.80 KiB |
| generated/large-array | 0.19 KiB | **0.03 KiB** | 5.72 MiB |
| generated/many-defs | 0.56 KiB | **0.03 KiB** | 17.64 KiB |
| generated/wide-object | 0.19 KiB | **0.03 KiB** | 650.63 KiB |
| large/api-catalog | 0.19 KiB | **0.03 KiB** | 2.98 KiB |
| large/circleci | 2.58 KiB | **0.03 KiB** | 1.31 KiB |
| massive/data-warehouse | 0.19 KiB | **0.03 KiB** | 13.13 KiB |
| massive/k8s-deployment | 0.19 KiB | **0.03 KiB** | 1.62 KiB |
| medium/user-profile | 0.19 KiB | **0.03 KiB** | 3.58 KiB |
| small/config | 0.19 KiB | **0.03 KiB** | 3.02 KiB |
| small/prettierrc | 0.77 KiB | **0.03 KiB** | 3.42 KiB |

### validate (invalid) — time

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 7.694 ± 0.112 µs | **2.808 ± 0.007 µs** | 138.216 ± 0.107 µs |
| generated/large-array | 17.906 ± 0.155 ms | **7.308 ± 0.069 ms** | 145.137 ± 1.598 ms |
| generated/many-defs | 1.746 ± 0.006 ms | **618.458 ± 1.975 µs** | 18.724 ± 0.276 ms |
| generated/wide-object | 170.591 ± 0.379 µs | **116.858 ± 0.585 µs** | 659.176 ± 0.783 µs |
| large/api-catalog | 662.979 ± 1.720 ns | **403.952 ± 1.246 ns** | 5.398 ± 0.015 µs |
| large/circleci | 129.297 ± 0.089 ns | 127.948 ± 2.161 ns | 426.585 ± 1.547 ns |
| massive/data-warehouse | 179.494 ± 0.154 µs | **136.922 ± 0.376 µs** | 923.101 ± 4.083 µs |
| massive/k8s-deployment | 176.093 ± 1.944 ns | **133.463 ± 0.337 ns** | 581.937 ± 1.517 ns |
| medium/user-profile | 370.622 ± 3.182 ns | **305.918 ± 0.786 ns** | 2.419 ± 0.023 µs |
| small/config | 289.487 ± 0.412 ns | **240.662 ± 0.449 ns** | 2.833 ± 0.010 µs |
| small/prettierrc | **528.292 ± 1.277 ns** | 898.284 ± 1.948 ns | 941.158 ± 2.992 ns |

### validate (invalid) — peak heap

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 2.97 KiB | **0.75 KiB** | 11.80 KiB |
| generated/large-array | 1.37 KiB | **0.46 KiB** | 5.72 MiB |
| generated/many-defs | 7.77 KiB | **0.77 KiB** | 17.64 KiB |
| generated/wide-object | 0.74 KiB | **0.41 KiB** | 650.63 KiB |
| large/api-catalog | 1.44 KiB | **0.83 KiB** | 3.15 KiB |
| large/circleci | 0.74 KiB | **0.40 KiB** | 1.26 KiB |
| massive/data-warehouse | 467.89 KiB | 111.78 KiB | **4.66 KiB** |
| massive/k8s-deployment | 0.81 KiB | **0.48 KiB** | 1.04 KiB |
| medium/user-profile | 1.30 KiB | **0.73 KiB** | 1.88 KiB |
| small/config | 1.30 KiB | **0.71 KiB** | 4.09 KiB |
| small/prettierrc | 2.38 KiB | 2.69 KiB | **1.64 KiB** |

## CLI (end to end)

### codegen — time

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | **8.443 ± 0.678 ms** | 105.998 ± 0.786 ms |
| generated/large-array | **1.747 ± 0.031 ms** | 85.956 ± 0.338 ms |
| generated/many-defs | **32.123 ± 0.199 ms** | 433.421 ± 1.074 ms |
| generated/wide-object | **139.461 ± 2.229 ms** | 504.179 ± 1.863 ms |
| large/api-catalog | **2.081 ± 0.042 ms** | 93.812 ± 0.450 ms |
| large/circleci | 3.950 ± 0.049 ms | — |
| massive/data-warehouse | **10.917 ± 0.081 ms** | 782.709 ± 2.978 ms |
| massive/k8s-deployment | **21.704 ± 0.183 ms** | 86.219 ± 1.276 ms |
| medium/user-profile | **1.794 ± 0.036 ms** | 88.348 ± 0.486 ms |
| small/config | **1.735 ± 0.046 ms** | 85.133 ± 0.375 ms |

### codegen — peak RSS

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | **24.02 ± 0.01 MiB** | 38.00 ± 0.01 MiB |
| generated/large-array | **3.66 MiB** | 33.30 ± 0.01 MiB |
| generated/many-defs | **56.45 ± 0.05 MiB** | 98.00 ± 0.01 MiB |
| generated/wide-object | **24.66 ± 0.12 MiB** | 113.92 ± 0.43 MiB |
| large/api-catalog | **4.48 ± 0.02 MiB** | 34.67 MiB |
| large/circleci | 10.52 ± 0.02 MiB | — |
| massive/data-warehouse | **32.05 MiB** | 175.91 ± 0.03 MiB |
| massive/k8s-deployment | 57.55 ± 0.01 MiB | **31.81 ± 0.01 MiB** |
| medium/user-profile | **3.73 MiB** | 33.56 ± 0.01 MiB |
| small/config | **3.45 MiB** | 32.94 ± 0.02 MiB |

### validate (valid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.420 ± 0.493 ms † | 3.867 ± 0.049 ms | **1.831 ± 0.042 ms** |
| generated/large-array | 58.443 ± 0.309 ms | **44.727 ± 0.827 ms** | 185.855 ± 2.361 ms |
| generated/many-defs | 22.840 ± 0.252 ms | **11.261 ± 0.100 ms** | 25.360 ± 0.222 ms |
| generated/wide-object | 27.727 ± 0.131 ms | 7.917 ± 0.098 ms | **3.618 ± 0.055 ms** |
| large/api-catalog | 3.034 ± 0.079 ms | 3.750 ± 0.040 ms | **1.564 ± 0.027 ms** |
| large/circleci | 6.705 ± 0.041 ms | 5.559 ± 0.142 ms | **1.894 ± 0.038 ms** |
| massive/data-warehouse | 49.488 ± 0.398 ms | 11.838 ± 0.117 ms | **4.013 ± 0.039 ms** |
| massive/k8s-deployment | 11.735 ± 0.202 ms | 275.777 ± 38.603 ms † | **2.274 ± 0.032 ms** |
| medium/user-profile | 2.862 ± 0.033 ms | 3.756 ± 0.097 ms | **1.527 ± 0.034 ms** |
| small/config | 2.791 ± 0.031 ms | 3.593 ± 0.052 ms | **1.513 ± 0.035 ms** |
| small/prettierrc | 3.008 ± 0.026 ms | 3.641 ± 0.060 ms | **1.551 ± 0.053 ms** |

### validate (valid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.78 MiB | 9.89 MiB | **3.80 MiB** |
| generated/large-array | **55.78 MiB** | 64.67 ± 0.01 MiB | 66.77 MiB |
| generated/many-defs | 19.03 MiB | 21.38 MiB | **11.92 ± 0.02 MiB** |
| generated/wide-object | 15.08 MiB | 15.27 ± 0.02 MiB | **6.14 MiB** |
| large/api-catalog | 5.86 MiB | 10.36 MiB | **3.20 MiB** |
| large/circleci | 9.02 ± 0.02 MiB | 13.05 ± 0.01 MiB | **4.12 ± 0.01 MiB** |
| massive/data-warehouse | 22.02 ± 0.01 MiB | 20.95 ± 0.02 MiB | **7.84 ± 0.01 MiB** |
| massive/k8s-deployment | 11.78 ± 0.04 MiB | 20.59 MiB | **4.94 ± 0.00 MiB** |
| medium/user-profile | 5.70 MiB | 10.09 ± 0.01 MiB | **3.12 MiB** |
| small/config | 5.67 ± 0.01 MiB | 9.86 MiB | **3.11 MiB** |
| small/prettierrc | 5.75 ± 0.01 MiB | 8.95 MiB | **3.14 MiB** |

### validate (invalid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.039 ± 0.051 ms | 3.869 ± 0.072 ms | **1.840 ± 0.038 ms** |
| generated/large-array | 58.455 ± 0.237 ms | **44.576 ± 0.310 ms** | 186.005 ± 1.229 ms |
| generated/many-defs | 22.980 ± 0.224 ms | **11.153 ± 0.112 ms** | 25.445 ± 0.213 ms |
| generated/wide-object | 27.750 ± 0.200 ms | 7.953 ± 0.130 ms | **3.637 ± 0.021 ms** |
| large/api-catalog | 3.013 ± 0.034 ms | 3.750 ± 0.046 ms | **1.549 ± 0.018 ms** |
| large/circleci | 6.650 ± 0.027 ms | 5.572 ± 0.116 ms | **1.867 ± 0.035 ms** |
| massive/data-warehouse | 50.909 ± 0.283 ms | 11.865 ± 0.174 ms | **4.202 ± 0.043 ms** |
| massive/k8s-deployment | 11.735 ± 0.089 ms | 294.517 ± 50.031 ms † | **2.352 ± 0.310 ms †** |
| medium/user-profile | 2.855 ± 0.020 ms | 3.686 ± 0.048 ms | **1.524 ± 0.021 ms** |
| small/config | 2.824 ± 0.035 ms | 3.698 ± 0.084 ms | **1.499 ± 0.026 ms** |
| small/prettierrc | 3.013 ± 0.035 ms | 3.627 ± 0.047 ms | **1.558 ± 0.031 ms** |

### validate (invalid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.73 ± 0.02 MiB | 10.02 ± 0.01 MiB | **3.80 ± 0.01 MiB** |
| generated/large-array | **55.78 ± 0.01 MiB** | 64.80 MiB | 66.80 MiB |
| generated/many-defs | 19.05 ± 0.01 MiB | 21.44 ± 0.01 MiB | **11.92 MiB** |
| generated/wide-object | 15.09 ± 0.01 MiB | 15.41 MiB | **6.16 MiB** |
| large/api-catalog | 5.86 ± 0.05 MiB | 10.50 MiB | **3.19 MiB** |
| large/circleci | 9.05 ± 0.01 MiB | 13.05 ± 0.02 MiB | **4.16 ± 0.01 MiB** |
| massive/data-warehouse | 22.75 ± 0.01 MiB | 21.05 ± 0.01 MiB | **7.56 ± 0.01 MiB** |
| massive/k8s-deployment | 11.70 ± 0.04 MiB | 20.53 ± 0.02 MiB | **4.92 MiB** |
| medium/user-profile | 5.73 MiB | 10.22 MiB | **3.12 MiB** |
| small/config | 5.72 ± 0.02 MiB | 10.02 ± 0.00 MiB | **3.12 MiB** |
| small/prettierrc | 5.81 MiB | 9.14 ± 0.01 MiB | **3.11 MiB** |

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
