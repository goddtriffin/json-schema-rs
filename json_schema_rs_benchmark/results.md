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
| generated/deep-nesting | **6.383 ± 0.035 ms** | 9.602 ± 0.053 ms |
| generated/large-array | **74.871 ± 0.236 µs** | 1.031 ± 0.016 ms |
| generated/many-defs | **38.358 ± 0.184 ms** | 122.570 ± 2.947 ms |
| generated/wide-object | **138.877 ± 1.225 ms** | 147.006 ± 0.326 ms |
| large/api-catalog | **208.905 ± 0.240 µs** | 3.101 ± 0.031 ms |
| large/circleci | 1.442 ± 0.003 ms | — |
| massive/data-warehouse | **9.632 ± 0.018 ms** | 269.952 ± 2.057 ms |
| massive/k8s-deployment | 19.055 ± 0.099 ms | **1.815 ± 0.003 ms** |
| medium/user-profile | **94.066 ± 0.088 µs** | 1.417 ± 0.003 ms |
| small/config | **29.959 ± 0.024 µs** | 509.403 ± 6.517 µs |

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
| generated/deep-nesting | 1.029 ± 0.003 ms | 206.992 ± 0.834 µs | **65.583 ± 0.098 µs** |
| generated/large-array | 45.004 ± 0.153 µs | 20.960 ± 0.052 µs | **4.306 ± 0.021 µs** |
| generated/many-defs | 13.805 ± 0.024 ms | 3.020 ± 0.035 ms | **691.439 ± 0.999 µs** |
| generated/wide-object | 23.307 ± 0.040 ms | 2.502 ± 0.005 ms | **789.395 ± 0.669 µs** |
| large/api-catalog | 178.956 ± 1.835 µs | 103.491 ± 0.069 µs | **15.463 ± 0.031 µs** |
| large/circleci | 2.876 ± 0.015 ms | 1.027 ± 0.020 ms | **258.837 ± 0.297 µs** |
| massive/data-warehouse | 45.095 ± 0.141 ms | 6.322 ± 0.093 ms | **1.244 ± 0.001 ms** |
| massive/k8s-deployment | 7.377 ± 0.020 ms | 1.599 ± 0.011 ms | **611.140 ± 0.857 µs** |
| medium/user-profile | 66.769 ± 0.337 µs | 40.033 ± 0.053 µs | **5.685 ± 0.031 µs** |
| small/config | 26.908 ± 0.079 µs | 18.957 ± 0.086 µs | **1.857 ± 0.006 µs** |
| small/prettierrc | 205.255 ± 0.511 µs | 110.412 ± 0.117 µs | **25.294 ± 0.040 µs** |

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
| generated/deep-nesting | 6.452 ± 0.036 µs | **2.434 ± 0.011 µs** | 139.591 ± 0.261 µs |
| generated/large-array | 17.861 ± 0.065 ms | **7.434 ± 0.167 ms** | 149.920 ± 0.922 ms |
| generated/many-defs | 1.709 ± 0.005 ms | **616.342 ± 7.626 µs** | 18.859 ± 0.190 ms |
| generated/wide-object | 167.106 ± 1.052 µs | **108.526 ± 1.075 µs** | 807.403 ± 2.068 µs |
| large/api-catalog | 558.347 ± 1.940 ns | **240.039 ± 1.264 ns** | 6.789 ± 0.028 µs |
| large/circleci | 2.287 ± 0.027 µs | **467.826 ± 0.540 ns** | 2.214 ± 0.003 µs |
| massive/data-warehouse | 69.242 ± 0.156 µs | **23.754 ± 0.104 µs** | 630.823 ± 1.916 µs |
| massive/k8s-deployment | 515.356 ± 4.033 ns | **216.113 ± 0.471 ns** | 4.670 ± 0.015 µs |
| medium/user-profile | 836.802 ± 3.185 ns | **371.250 ± 1.551 ns** | 6.067 ± 0.043 µs |
| small/config | 222.824 ± 0.411 ns | **112.665 ± 1.375 ns** | 2.824 ± 0.011 µs |
| small/prettierrc | 718.523 ± 5.287 ns | **173.819 ± 1.402 ns** | 3.179 ± 0.004 µs |

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
| generated/deep-nesting | 7.694 ± 0.014 µs | **2.804 ± 0.007 µs** | 140.152 ± 0.195 µs |
| generated/large-array | 17.930 ± 0.029 ms | **7.430 ± 0.028 ms** | 148.917 ± 1.209 ms |
| generated/many-defs | 1.729 ± 0.019 ms | **620.320 ± 2.913 µs** | 18.850 ± 0.225 ms |
| generated/wide-object | 168.532 ± 0.276 µs | **110.432 ± 0.376 µs** | 800.530 ± 7.253 µs |
| large/api-catalog | 646.083 ± 1.818 ns | **393.240 ± 0.907 ns** | 5.382 ± 0.008 µs |
| large/circleci | **125.065 ± 0.126 ns** | 130.024 ± 0.490 ns | 429.205 ± 0.864 ns |
| massive/data-warehouse | 178.088 ± 2.341 µs | **140.549 ± 0.979 µs** | 925.492 ± 13.954 µs |
| massive/k8s-deployment | 174.387 ± 1.474 ns | **134.947 ± 0.217 ns** | 579.483 ± 2.246 ns |
| medium/user-profile | 364.826 ± 0.786 ns | **305.302 ± 0.754 ns** | 2.404 ± 0.017 µs |
| small/config | 286.301 ± 1.624 ns | **243.368 ± 1.001 ns** | 2.854 ± 0.015 µs |
| small/prettierrc | **532.175 ± 4.990 ns** | 905.911 ± 7.410 ns | 900.303 ± 1.085 ns |

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
| generated/deep-nesting | **11.184 ± 0.204 ms** | 108.298 ± 1.517 ms |
| generated/large-array | **1.809 ± 0.041 ms** | 87.405 ± 0.679 ms |
| generated/many-defs | **45.362 ± 0.245 ms** | 439.280 ± 3.714 ms |
| generated/wide-object | **144.470 ± 2.032 ms** | 512.771 ± 3.557 ms |
| large/api-catalog | **2.194 ± 0.048 ms** | 94.658 ± 0.623 ms |
| large/circleci | 4.234 ± 0.187 ms | — |
| massive/data-warehouse | **14.092 ± 0.296 ms** | 792.247 ± 3.925 ms |
| massive/k8s-deployment | **28.274 ± 0.591 ms** | 86.732 ± 1.144 ms |
| medium/user-profile | **1.853 ± 0.055 ms** | 88.917 ± 0.959 ms |
| small/config | **1.724 ± 0.051 ms** | 85.370 ± 0.571 ms |

### codegen — peak RSS

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | 45.58 ± 0.00 MiB | **37.98 ± 0.02 MiB** |
| generated/large-array | **3.97 ± 0.01 MiB** | 33.31 ± 0.01 MiB |
| generated/many-defs | 116.11 ± 0.03 MiB | **98.08 ± 0.02 MiB** |
| generated/wide-object | **39.50 ± 0.09 MiB** | 115.19 ± 0.63 MiB |
| large/api-catalog | **5.05 MiB** | 34.59 ± 0.05 MiB |
| large/circleci | 13.30 MiB | — |
| massive/data-warehouse | **38.66 MiB** | 177.00 ± 0.40 MiB |
| massive/k8s-deployment | 96.97 MiB | **31.80 ± 0.01 MiB** |
| medium/user-profile | **4.03 ± 0.01 MiB** | 33.58 MiB |
| small/config | **3.44 MiB** | 32.94 ± 0.02 MiB |

### validate (valid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.415 ± 0.393 ms | 4.154 ± 0.164 ms | **1.908 ± 0.078 ms** |
| generated/large-array | 58.877 ± 1.386 ms | **44.876 ± 0.411 ms** | 189.650 ± 1.112 ms |
| generated/many-defs | 23.027 ± 0.755 ms | **11.610 ± 0.468 ms** | 26.215 ± 0.180 ms |
| generated/wide-object | 27.875 ± 0.276 ms | 8.029 ± 0.118 ms | **4.139 ± 0.047 ms** |
| large/api-catalog | 3.183 ± 0.073 ms | 3.895 ± 0.171 ms | **1.705 ± 0.059 ms** |
| large/circleci | 6.879 ± 0.077 ms | 5.738 ± 0.055 ms | **2.042 ± 0.038 ms** |
| massive/data-warehouse | 50.089 ± 0.378 ms | 11.916 ± 0.082 ms | **4.437 ± 0.092 ms** |
| massive/k8s-deployment | 11.927 ± 0.177 ms | 336.792 ± 58.759 ms † | **2.731 ± 0.057 ms** |
| medium/user-profile | 2.936 ± 0.066 ms | 3.721 ± 0.147 ms | **1.545 ± 0.035 ms** |
| small/config | 2.862 ± 0.088 ms | 3.640 ± 0.218 ms | **1.512 ± 0.035 ms** |
| small/prettierrc | 3.186 ± 0.097 ms | 3.782 ± 0.167 ms | **1.635 ± 0.082 ms** |

### validate (valid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.89 ± 0.03 MiB | 9.88 MiB | **4.14 ± 0.00 MiB** |
| generated/large-array | **55.80 ± 0.02 MiB** | 64.69 MiB | 68.48 MiB |
| generated/many-defs | 20.70 ± 0.79 MiB | 21.36 ± 0.01 MiB | **14.45 MiB** |
| generated/wide-object | 15.12 ± 0.01 MiB | 15.27 MiB | **7.16 ± 0.01 MiB** |
| large/api-catalog | 5.86 MiB | 10.34 ± 0.00 MiB | **3.30 MiB** |
| large/circleci | 9.20 ± 0.01 MiB | 13.08 ± 0.02 MiB | **4.50 MiB** |
| massive/data-warehouse | 22.08 MiB | 20.98 ± 0.02 MiB | **7.89 MiB** |
| massive/k8s-deployment | 11.88 ± 0.02 MiB | 20.45 ± 0.01 MiB | **6.47 ± 0.01 MiB** |
| medium/user-profile | 5.70 MiB | 10.06 ± 0.01 MiB | **3.22 MiB** |
| small/config | 5.69 ± 0.02 MiB | 9.88 ± 0.01 MiB | **3.14 MiB** |
| small/prettierrc | 5.80 ± 0.01 MiB | 8.94 ± 0.02 MiB | **3.31 ± 0.01 MiB** |

### validate (invalid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.146 ± 0.129 ms | 4.094 ± 0.073 ms | **1.911 ± 0.033 ms** |
| generated/large-array | 58.933 ± 0.736 ms | **44.623 ± 0.511 ms** | 189.242 ± 1.056 ms |
| generated/many-defs | 23.108 ± 0.183 ms | **11.333 ± 0.107 ms** | 26.184 ± 0.248 ms |
| generated/wide-object | 27.843 ± 0.135 ms | 8.060 ± 0.093 ms | **4.140 ± 0.203 ms** |
| large/api-catalog | 3.103 ± 0.047 ms | 3.973 ± 0.038 ms | **1.607 ± 0.041 ms** |
| large/circleci | 6.827 ± 0.062 ms | 5.724 ± 0.180 ms | **2.008 ± 0.034 ms** |
| massive/data-warehouse | 51.247 ± 0.283 ms | 11.911 ± 0.082 ms | **4.629 ± 0.162 ms** |
| massive/k8s-deployment | 11.843 ± 0.095 ms | 322.379 ± 41.713 ms † | **2.776 ± 0.066 ms** |
| medium/user-profile | 2.871 ± 0.050 ms | 3.800 ± 0.083 ms | **1.519 ± 0.019 ms** |
| small/config | 2.882 ± 0.057 ms | 3.673 ± 0.063 ms | **1.515 ± 0.038 ms** |
| small/prettierrc | 3.083 ± 0.109 ms | 3.789 ± 0.048 ms | **1.579 ± 0.031 ms** |

### validate (invalid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.89 MiB | 10.02 MiB | **4.19 ± 0.01 MiB** |
| generated/large-array | **55.80 ± 0.01 MiB** | 64.88 MiB | 68.47 ± 0.02 MiB |
| generated/many-defs | 19.09 MiB | 21.44 ± 0.01 MiB | **14.50 MiB** |
| generated/wide-object | 15.44 MiB | 15.42 MiB | **7.17 ± 0.14 MiB** |
| large/api-catalog | 5.86 MiB | 10.50 ± 0.01 MiB | **3.31 MiB** |
| large/circleci | 9.17 ± 0.03 MiB | 13.03 ± 0.03 MiB | **4.48 MiB** |
| massive/data-warehouse | 22.62 ± 0.07 MiB | 21.06 ± 0.03 MiB | **7.66 MiB** |
| massive/k8s-deployment | 11.81 ± 0.01 MiB | 20.53 MiB | **6.44 ± 0.01 MiB** |
| medium/user-profile | 5.70 MiB | 10.23 ± 0.02 MiB | **3.22 MiB** |
| small/config | 5.67 MiB | 10.00 MiB | **3.19 MiB** |
| small/prettierrc | 5.81 ± 0.01 MiB | 9.14 ± 0.00 MiB | **3.28 MiB** |

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
