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
| in-process | validate (invalid) | time | jsonschema | 10 / 11 |
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
| generated/deep-nesting | **4.529 ± 0.067 ms** | 9.749 ± 0.126 ms |
| generated/large-array | **62.627 ± 0.189 µs** | 1.029 ± 0.003 ms |
| generated/many-defs | **27.054 ± 0.068 ms** | 122.607 ± 2.027 ms |
| generated/wide-object | **137.285 ± 1.131 ms** | 147.710 ± 0.779 ms |
| large/api-catalog | **195.564 ± 0.560 µs** | 3.100 ± 0.007 ms |
| large/circleci | 1.612 ± 0.009 ms | — |
| massive/data-warehouse | **7.812 ± 0.063 ms** | 268.545 ± 3.320 ms |
| massive/k8s-deployment | 14.976 ± 0.044 ms | **1.810 ± 0.006 ms** |
| medium/user-profile | **81.575 ± 0.645 µs** | 1.428 ± 0.019 ms |
| small/config | **27.365 ± 0.484 µs** | 509.065 ± 3.655 µs |

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
| generated/deep-nesting | 1.023 ± 0.017 ms | 206.618 ± 2.532 µs | **70.150 ± 0.162 µs** |
| generated/large-array | 44.739 ± 0.073 µs | 20.557 ± 0.031 µs | **6.148 ± 0.012 µs** |
| generated/many-defs | 13.931 ± 0.065 ms | 3.004 ± 0.005 ms | **1.057 ± 0.013 ms** |
| generated/wide-object | 23.333 ± 0.032 ms | 2.507 ± 0.008 ms | **818.212 ± 4.588 µs** |
| large/api-catalog | 177.299 ± 0.272 µs | 103.043 ± 0.141 µs | **23.998 ± 0.040 µs** |
| large/circleci | 2.860 ± 0.032 ms | 1.017 ± 0.001 ms | **301.560 ± 0.514 µs** |
| massive/data-warehouse | 45.100 ± 0.115 ms | 6.265 ± 0.006 ms | **1.560 ± 0.004 ms** |
| massive/k8s-deployment | 7.376 ± 0.019 ms | 1.589 ± 0.002 ms | **495.831 ± 0.992 µs** |
| medium/user-profile | 66.913 ± 0.093 µs | 39.714 ± 0.202 µs | **9.122 ± 0.028 µs** |
| small/config | 26.692 ± 0.038 µs | 18.909 ± 0.042 µs | **3.274 ± 0.017 µs** |
| small/prettierrc | 204.298 ± 0.344 µs | 111.123 ± 0.234 µs | **32.823 ± 0.102 µs** |

### schema compile — peak heap

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 636.04 KiB | 534.41 KiB | **118.16 KiB** |
| generated/large-array | 66.67 KiB | 34.10 KiB | **17.48 KiB** |
| generated/many-defs | 8.10 MiB | 3.88 MiB | **2.32 MiB** |
| generated/wide-object | 13.73 MiB | 4.03 MiB | **2.18 MiB** |
| large/api-catalog | 266.71 KiB | 151.24 KiB | **52.47 KiB** |
| large/circleci | 3.85 MiB | 1.75 MiB | **739.61 KiB** |
| massive/data-warehouse | 16.73 MiB | 9.42 MiB | **3.40 MiB** |
| massive/k8s-deployment | 7.45 MiB | 3.28 MiB | **1.22 MiB** |
| medium/user-profile | 121.82 KiB | 59.13 KiB | **22.68 KiB** |
| small/config | 38.35 KiB | 24.08 KiB | **8.49 KiB** |
| small/prettierrc | 476.85 KiB | 261.04 KiB | **98.00 KiB** |

### validate (valid) — time

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.668 ± 0.024 µs | **2.437 ± 0.010 µs** | 138.323 ± 0.202 µs |
| generated/large-array | 17.971 ± 0.031 ms | **7.458 ± 0.096 ms** | 145.351 ± 0.766 ms |
| generated/many-defs | 1.733 ± 0.006 ms | **617.203 ± 4.059 µs** | 18.650 ± 0.144 ms |
| generated/wide-object | 172.432 ± 0.593 µs | **107.778 ± 2.241 µs** | 662.047 ± 3.501 µs |
| large/api-catalog | 562.337 ± 1.480 ns | **240.760 ± 1.250 ns** | 6.752 ± 0.016 µs |
| large/circleci | 2.172 ± 0.004 µs | **464.909 ± 1.246 ns** | 2.062 ± 0.007 µs |
| massive/data-warehouse | 70.147 ± 0.400 µs | **23.564 ± 0.040 µs** | 620.816 ± 2.354 µs |
| massive/k8s-deployment | 516.620 ± 1.269 ns | **209.118 ± 0.385 ns** | 4.625 ± 0.014 µs |
| medium/user-profile | 844.333 ± 1.966 ns | **372.548 ± 1.423 ns** | 5.969 ± 0.051 µs |
| small/config | 225.738 ± 0.953 ns | **112.529 ± 0.501 ns** | 2.875 ± 0.007 µs |
| small/prettierrc | 719.307 ± 3.367 ns | **174.373 ± 1.233 ns** | 2.954 ± 0.007 µs |

### validate (valid) — peak heap

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 2.25 KiB | **0.03 KiB** | 11.86 KiB |
| generated/large-array | 0.19 KiB | **0.03 KiB** | 5.97 MiB |
| generated/many-defs | 0.56 KiB | **0.03 KiB** | 18.42 KiB |
| generated/wide-object | 0.19 KiB | **0.03 KiB** | 697.51 KiB |
| large/api-catalog | 0.19 KiB | **0.03 KiB** | 3.01 KiB |
| large/circleci | 2.58 KiB | **0.03 KiB** | 1.37 KiB |
| massive/data-warehouse | 0.19 KiB | **0.03 KiB** | 13.77 KiB |
| massive/k8s-deployment | 0.19 KiB | **0.03 KiB** | 1.68 KiB |
| medium/user-profile | 0.19 KiB | **0.03 KiB** | 3.82 KiB |
| small/config | 0.19 KiB | **0.03 KiB** | 3.09 KiB |
| small/prettierrc | 0.77 KiB | **0.03 KiB** | 3.68 KiB |

### validate (invalid) — time

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 7.774 ± 0.008 µs | **2.780 ± 0.005 µs** | 140.330 ± 0.212 µs |
| generated/large-array | 17.986 ± 0.035 ms | **7.517 ± 0.102 ms** | 145.593 ± 0.232 ms |
| generated/many-defs | 1.751 ± 0.007 ms | **618.673 ± 1.266 µs** | 18.697 ± 0.081 ms |
| generated/wide-object | 175.597 ± 0.478 µs | **108.658 ± 1.071 µs** | 660.603 ± 3.610 µs |
| large/api-catalog | 670.866 ± 1.021 ns | **394.948 ± 1.312 ns** | 5.418 ± 0.009 µs |
| large/circleci | 137.006 ± 0.401 ns | **131.621 ± 0.518 ns** | 407.523 ± 1.507 ns |
| massive/data-warehouse | 182.709 ± 0.268 µs | **139.616 ± 0.265 µs** | 909.780 ± 2.014 µs |
| massive/k8s-deployment | 183.919 ± 0.236 ns | **132.497 ± 0.227 ns** | 580.990 ± 1.937 ns |
| medium/user-profile | 375.570 ± 0.595 ns | **317.680 ± 1.341 ns** | 2.408 ± 0.013 µs |
| small/config | 293.656 ± 0.391 ns | **247.730 ± 1.026 ns** | 2.867 ± 0.006 µs |
| small/prettierrc | **527.952 ± 1.756 ns** | 898.384 ± 8.684 ns | 918.369 ± 1.948 ns |

### validate (invalid) — peak heap

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 2.97 KiB | **0.75 KiB** | 11.86 KiB |
| generated/large-array | 1.37 KiB | **0.46 KiB** | 5.97 MiB |
| generated/many-defs | 7.77 KiB | **0.77 KiB** | 18.42 KiB |
| generated/wide-object | 0.74 KiB | **0.41 KiB** | 697.51 KiB |
| large/api-catalog | 1.44 KiB | **0.83 KiB** | 3.18 KiB |
| large/circleci | 0.74 KiB | **0.40 KiB** | 1.29 KiB |
| massive/data-warehouse | 467.89 KiB | 111.78 KiB | **4.72 KiB** |
| massive/k8s-deployment | 0.81 KiB | **0.48 KiB** | 1.07 KiB |
| medium/user-profile | 1.30 KiB | **0.73 KiB** | 1.91 KiB |
| small/config | 1.30 KiB | **0.71 KiB** | 4.12 KiB |
| small/prettierrc | 2.38 KiB | 2.69 KiB | **1.73 KiB** |

## CLI (end to end)

### codegen — time

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | **8.623 ± 0.285 ms** | 108.760 ± 2.254 ms |
| generated/large-array | **1.883 ± 0.040 ms** | 87.548 ± 0.662 ms |
| generated/many-defs | **32.804 ± 0.210 ms** | 444.272 ± 2.484 ms |
| generated/wide-object | **142.186 ± 3.143 ms** | 514.514 ± 5.609 ms |
| large/api-catalog | **2.561 ± 0.152 ms** | 109.195 ± 4.084 ms |
| large/circleci | 5.678 ± 0.405 ms | — |
| massive/data-warehouse | **12.730 ± 0.464 ms** | 796.123 ± 10.780 ms |
| massive/k8s-deployment | **22.901 ± 0.960 ms** | 87.516 ± 2.656 ms |
| medium/user-profile | **1.874 ± 0.037 ms** | 92.355 ± 3.006 ms |
| small/config | **1.918 ± 0.160 ms** | 87.237 ± 3.243 ms |

### codegen — peak RSS

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | **24.27 MiB** | 37.98 MiB |
| generated/large-array | **3.83 ± 0.01 MiB** | 33.28 MiB |
| generated/many-defs | **57.16 MiB** | 98.11 ± 0.06 MiB |
| generated/wide-object | **25.05 ± 0.28 MiB** | 115.55 ± 0.29 MiB |
| large/api-catalog | **4.69 ± 0.01 MiB** | 34.69 ± 0.04 MiB |
| large/circleci | 10.75 MiB | — |
| massive/data-warehouse | **32.80 ± 0.01 MiB** | 176.03 ± 0.05 MiB |
| massive/k8s-deployment | 58.22 MiB | **31.80 ± 0.02 MiB** |
| medium/user-profile | **3.92 MiB** | 33.59 ± 0.01 MiB |
| small/config | **3.61 MiB** | 32.94 MiB |

### validate (valid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.445 ± 0.391 ms | 4.233 ± 0.262 ms | **1.979 ± 0.033 ms** |
| generated/large-array | 58.703 ± 0.248 ms | **44.754 ± 0.832 ms** | 185.236 ± 1.855 ms |
| generated/many-defs | 23.207 ± 0.326 ms | **11.511 ± 0.178 ms** | 25.932 ± 0.355 ms |
| generated/wide-object | 27.919 ± 0.712 ms | 8.082 ± 0.091 ms | **4.027 ± 0.038 ms** |
| large/api-catalog | 4.360 ± 0.321 ms | 5.324 ± 0.570 ms † | **1.995 ± 0.250 ms †** |
| large/circleci | 6.971 ± 0.141 ms | 7.167 ± 0.673 ms | **2.928 ± 0.200 ms** |
| massive/data-warehouse | 50.238 ± 1.559 ms | 12.217 ± 0.298 ms | **4.837 ± 0.063 ms** |
| massive/k8s-deployment | 11.995 ± 0.143 ms | 353.163 ± 54.830 ms † | **2.561 ± 0.064 ms** |
| medium/user-profile | 2.948 ± 0.078 ms | 3.856 ± 0.071 ms | **1.678 ± 0.031 ms** |
| small/config | 2.986 ± 0.227 ms | 3.828 ± 0.093 ms | **1.625 ± 0.066 ms** |
| small/prettierrc | 3.151 ± 0.170 ms | 4.009 ± 0.201 ms | **1.667 ± 0.093 ms** |

### validate (valid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.81 MiB | 9.89 MiB | **4.03 MiB** |
| generated/large-array | **55.80 MiB** | 64.67 ± 0.01 MiB | 67.30 ± 0.01 MiB |
| generated/many-defs | 19.11 MiB | 21.36 ± 0.02 MiB | **12.38 ± 0.02 MiB** |
| generated/wide-object | 15.09 ± 0.01 MiB | 15.30 ± 0.01 MiB | **6.73 MiB** |
| large/api-catalog | 5.98 MiB | 10.36 ± 0.01 MiB | **3.48 ± 0.02 MiB** |
| large/circleci | 9.09 ± 0.03 MiB | 13.09 MiB | **4.59 MiB** |
| massive/data-warehouse | 22.05 ± 0.04 MiB | 21.00 ± 0.14 MiB | **8.44 ± 0.01 MiB** |
| massive/k8s-deployment | 11.86 ± 0.02 MiB | 20.50 ± 0.04 MiB | **5.30 MiB** |
| medium/user-profile | 5.75 ± 0.01 MiB | 10.09 MiB | **3.39 MiB** |
| small/config | 5.77 MiB | 9.84 MiB | **3.33 MiB** |
| small/prettierrc | 5.83 ± 0.01 MiB | 8.95 MiB | **3.39 MiB** |

### validate (invalid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.225 ± 0.133 ms | 4.121 ± 0.117 ms | **2.024 ± 0.075 ms** |
| generated/large-array | 58.759 ± 0.416 ms | **45.141 ± 0.470 ms** | 185.391 ± 0.571 ms |
| generated/many-defs | 22.970 ± 0.944 ms | **11.584 ± 0.109 ms** | 26.122 ± 0.296 ms |
| generated/wide-object | 28.034 ± 0.882 ms | 8.096 ± 0.129 ms | **4.056 ± 0.080 ms** |
| large/api-catalog | 4.354 ± 0.309 ms | 5.725 ± 0.487 ms | **2.065 ± 0.234 ms †** |
| large/circleci | 6.934 ± 0.087 ms | 6.677 ± 0.186 ms | **2.977 ± 0.220 ms** |
| massive/data-warehouse | 50.957 ± 0.536 ms | 12.909 ± 0.665 ms | **5.083 ± 0.237 ms** |
| massive/k8s-deployment | 11.911 ± 0.044 ms | 366.700 ± 42.865 ms † | **2.503 ± 0.038 ms** |
| medium/user-profile | 2.949 ± 0.033 ms | 3.826 ± 0.096 ms | **1.575 ± 0.047 ms** |
| small/config | 2.915 ± 0.148 ms | 3.748 ± 0.053 ms | **1.600 ± 0.030 ms** |
| small/prettierrc | 3.104 ± 0.058 ms | 3.782 ± 0.064 ms | **1.619 ± 0.079 ms** |

### validate (invalid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.81 ± 0.03 MiB | 10.03 ± 0.02 MiB | **4.05 ± 0.01 MiB** |
| generated/large-array | **55.80 ± 0.01 MiB** | 64.83 ± 0.02 MiB | 67.31 ± 0.03 MiB |
| generated/many-defs | 20.48 ± 0.58 MiB | 21.41 ± 0.01 MiB | **12.48 ± 0.01 MiB** |
| generated/wide-object | 15.12 ± 0.02 MiB | 15.39 ± 0.01 MiB | **6.77 ± 0.02 MiB** |
| large/api-catalog | 6.08 MiB | 10.50 ± 0.01 MiB | **3.47 MiB** |
| large/circleci | 9.11 ± 0.01 MiB | 13.09 MiB | **4.56 ± 0.02 MiB** |
| massive/data-warehouse | 22.66 ± 0.01 MiB | 21.09 MiB | **8.48 ± 0.15 MiB** |
| massive/k8s-deployment | 11.81 ± 0.01 MiB | 20.64 MiB | **5.30 ± 0.01 MiB** |
| medium/user-profile | 5.75 MiB | 10.22 MiB | **3.41 MiB** |
| small/config | 5.77 MiB | 10.00 ± 0.01 MiB | **3.38 MiB** |
| small/prettierrc | 5.83 MiB | 9.14 ± 0.01 MiB | **3.38 ± 0.01 MiB** |

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
