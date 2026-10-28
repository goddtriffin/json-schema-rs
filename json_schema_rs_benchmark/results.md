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
| generated/deep-nesting | **6.196 ± 0.034 ms** | 9.623 ± 0.032 ms |
| generated/large-array | **73.600 ± 0.055 µs** | 1.036 ± 0.005 ms |
| generated/many-defs | **36.968 ± 0.108 ms** | 122.744 ± 1.438 ms |
| generated/wide-object | **137.627 ± 1.197 ms** | 147.732 ± 0.311 ms |
| large/api-catalog | **205.859 ± 0.479 µs** | 3.102 ± 0.015 ms |
| large/circleci | 1.385 ± 0.002 ms | — |
| massive/data-warehouse | **9.367 ± 0.027 ms** | 268.807 ± 0.879 ms |
| massive/k8s-deployment | 18.627 ± 0.065 ms | **1.817 ± 0.004 ms** |
| medium/user-profile | **92.467 ± 0.880 µs** | 1.423 ± 0.002 ms |
| small/config | **29.516 ± 0.093 µs** | 509.406 ± 0.821 µs |

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
| generated/deep-nesting | 1.023 ± 0.016 ms | 207.430 ± 0.311 µs | **64.436 ± 0.041 µs** |
| generated/large-array | 44.815 ± 0.798 µs | 20.977 ± 0.088 µs | **4.353 ± 0.017 µs** |
| generated/many-defs | 13.626 ± 0.047 ms | 3.013 ± 0.007 ms | **708.420 ± 11.376 µs** |
| generated/wide-object | 23.174 ± 0.095 ms | 2.535 ± 0.009 ms | **768.368 ± 3.161 µs** |
| large/api-catalog | 177.983 ± 1.287 µs | 102.923 ± 0.196 µs | **15.684 ± 0.034 µs** |
| large/circleci | 2.855 ± 0.014 ms | 1.016 ± 0.001 ms | **255.120 ± 0.506 µs** |
| massive/data-warehouse | 43.585 ± 0.366 ms | 6.293 ± 0.007 ms | **1.261 ± 0.001 ms** |
| massive/k8s-deployment | 7.299 ± 0.013 ms | 1.612 ± 0.002 ms | **610.147 ± 0.621 µs** |
| medium/user-profile | 66.825 ± 0.092 µs | 40.494 ± 0.062 µs | **5.695 ± 0.022 µs** |
| small/config | 27.018 ± 0.105 µs | 19.311 ± 0.029 µs | **1.838 ± 0.008 µs** |
| small/prettierrc | 204.099 ± 0.681 µs | 111.459 ± 0.169 µs | **25.385 ± 0.051 µs** |

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
| generated/deep-nesting | 6.798 ± 0.015 µs | **2.428 ± 0.011 µs** | 139.931 ± 0.211 µs |
| generated/large-array | 18.007 ± 0.073 ms | **7.387 ± 0.030 ms** | 146.891 ± 0.410 ms |
| generated/many-defs | 1.722 ± 0.011 ms | **619.378 ± 1.436 µs** | 18.875 ± 0.103 ms |
| generated/wide-object | 169.742 ± 0.265 µs | **112.020 ± 2.519 µs** | 794.601 ± 2.855 µs |
| large/api-catalog | 560.238 ± 1.254 ns | **240.006 ± 1.725 ns** | 6.777 ± 0.026 µs |
| large/circleci | 2.185 ± 0.009 µs | **468.795 ± 3.888 ns** | 2.198 ± 0.012 µs |
| massive/data-warehouse | 69.307 ± 0.318 µs | **23.767 ± 0.055 µs** | 610.887 ± 1.753 µs |
| massive/k8s-deployment | 518.430 ± 0.907 ns | **217.575 ± 0.799 ns** | 4.650 ± 0.067 µs |
| medium/user-profile | 836.437 ± 1.787 ns | **370.589 ± 1.080 ns** | 5.903 ± 0.017 µs |
| small/config | 225.531 ± 0.429 ns | **113.080 ± 0.469 ns** | 2.795 ± 0.027 µs |
| small/prettierrc | 714.312 ± 1.930 ns | **174.104 ± 0.969 ns** | 3.084 ± 0.009 µs |

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
| generated/deep-nesting | 7.785 ± 0.012 µs | **2.775 ± 0.004 µs** | 140.513 ± 0.176 µs |
| generated/large-array | 18.055 ± 0.034 ms | **7.372 ± 0.120 ms** | 146.523 ± 0.349 ms |
| generated/many-defs | 1.731 ± 0.031 ms | **621.290 ± 2.696 µs** | 18.783 ± 0.055 ms |
| generated/wide-object | 170.674 ± 0.265 µs | **112.400 ± 0.424 µs** | 788.764 ± 1.894 µs |
| large/api-catalog | 658.887 ± 3.181 ns | **393.956 ± 1.375 ns** | 5.361 ± 0.011 µs |
| large/circleci | 127.896 ± 0.197 ns | 130.235 ± 0.236 ns | 402.121 ± 0.989 ns |
| massive/data-warehouse | 183.231 ± 2.000 µs | **138.082 ± 1.325 µs** | 887.824 ± 1.202 µs |
| massive/k8s-deployment | 179.880 ± 0.215 ns | **134.235 ± 0.326 ns** | 558.847 ± 1.618 ns |
| medium/user-profile | 368.700 ± 1.053 ns | **305.799 ± 1.115 ns** | 2.381 ± 0.013 µs |
| small/config | 290.178 ± 3.049 ns | **243.153 ± 1.200 ns** | 2.780 ± 0.010 µs |
| small/prettierrc | **527.928 ± 1.074 ns** | 926.141 ± 2.729 ns | 876.811 ± 2.153 ns |

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
| generated/deep-nesting | **11.003 ± 0.236 ms** | 107.263 ± 0.392 ms |
| generated/large-array | **1.773 ± 0.034 ms** | 87.110 ± 1.802 ms |
| generated/many-defs | **44.469 ± 0.853 ms** | 440.699 ± 3.239 ms |
| generated/wide-object | **143.495 ± 1.419 ms** | 507.849 ± 3.203 ms |
| large/api-catalog | **2.126 ± 0.044 ms** | 95.113 ± 1.471 ms |
| large/circleci | 4.080 ± 0.086 ms | — |
| massive/data-warehouse | **13.752 ± 0.167 ms** | 787.139 ± 2.923 ms |
| massive/k8s-deployment | **27.412 ± 0.188 ms** | 85.516 ± 0.572 ms |
| medium/user-profile | **1.821 ± 0.049 ms** | 88.331 ± 1.470 ms |
| small/config | **1.713 ± 0.027 ms** | 84.893 ± 1.065 ms |

### codegen — peak RSS

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | 45.39 ± 0.01 MiB | **37.98 MiB** |
| generated/large-array | **3.81 ± 0.02 MiB** | 33.33 MiB |
| generated/many-defs | 115.09 ± 0.02 MiB | **98.11 MiB** |
| generated/wide-object | **38.81 ± 0.08 MiB** | 115.50 ± 0.08 MiB |
| large/api-catalog | **4.84 MiB** | 34.69 ± 0.01 MiB |
| large/circleci | 12.66 ± 0.01 MiB | — |
| massive/data-warehouse | **36.92 ± 0.01 MiB** | 177.23 ± 0.03 MiB |
| massive/k8s-deployment | 95.91 ± 0.03 MiB | **31.78 ± 0.01 MiB** |
| medium/user-profile | **3.83 MiB** | 33.58 MiB |
| small/config | **3.34 MiB** | 32.89 ± 0.05 MiB |

### validate (valid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.472 ± 0.488 ms † | 4.072 ± 0.143 ms | **1.867 ± 0.061 ms** |
| generated/large-array | 58.568 ± 0.334 ms | **44.711 ± 0.387 ms** | 188.633 ± 1.347 ms |
| generated/many-defs | 22.971 ± 0.301 ms | **11.352 ± 0.131 ms** | 26.152 ± 1.883 ms |
| generated/wide-object | 27.769 ± 0.218 ms | 7.883 ± 0.147 ms | **3.964 ± 0.041 ms** |
| large/api-catalog | 3.031 ± 0.046 ms | 3.819 ± 0.122 ms | **1.555 ± 0.030 ms** |
| large/circleci | 6.755 ± 0.112 ms | 5.835 ± 0.465 ms | **1.953 ± 0.028 ms** |
| massive/data-warehouse | 49.940 ± 0.312 ms | 11.750 ± 0.101 ms | **4.198 ± 0.047 ms** |
| massive/k8s-deployment | 11.912 ± 0.206 ms | 268.847 ± 85.579 ms † | **2.610 ± 0.072 ms** |
| medium/user-profile | 2.894 ± 0.079 ms | 3.623 ± 0.308 ms | **1.549 ± 0.034 ms** |
| small/config | 2.830 ± 0.051 ms | 3.576 ± 0.383 ms † | **1.513 ± 0.037 ms** |
| small/prettierrc | 3.183 ± 0.182 ms | 3.771 ± 0.072 ms | **1.590 ± 0.060 ms** |

### validate (valid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.89 ± 0.01 MiB | 9.91 ± 0.01 MiB | **3.95 ± 0.02 MiB** |
| generated/large-array | **55.77 MiB** | 64.67 ± 0.01 MiB | 68.16 MiB |
| generated/many-defs | 19.06 MiB | 21.39 ± 0.02 MiB | **14.25 ± 0.02 MiB** |
| generated/wide-object | 15.09 ± 0.01 MiB | 15.30 ± 0.02 MiB | **6.97 MiB** |
| large/api-catalog | 5.86 MiB | 10.34 ± 0.01 MiB | **3.06 MiB** |
| large/circleci | 9.22 ± 0.07 MiB | 13.28 ± 0.12 MiB | **4.28 MiB** |
| massive/data-warehouse | 21.97 MiB | 20.97 ± 0.01 MiB | **7.58 MiB** |
| massive/k8s-deployment | 11.78 MiB | 20.52 MiB | **6.20 MiB** |
| medium/user-profile | 5.70 MiB | 10.06 MiB | **2.97 MiB** |
| small/config | 5.69 MiB | 9.84 ± 0.01 MiB | **2.94 MiB** |
| small/prettierrc | 5.86 ± 0.01 MiB | 8.95 ± 0.02 MiB | **3.14 ± 0.00 MiB** |

### validate (invalid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.060 ± 0.120 ms | 3.987 ± 0.072 ms | **1.849 ± 0.031 ms** |
| generated/large-array | 58.644 ± 0.219 ms | **44.586 ± 0.597 ms** | 187.685 ± 1.314 ms |
| generated/many-defs | 22.975 ± 0.254 ms | **11.373 ± 0.168 ms** | 25.942 ± 0.204 ms |
| generated/wide-object | 28.032 ± 1.176 ms | 7.925 ± 0.077 ms | **3.980 ± 0.116 ms** |
| large/api-catalog | 3.061 ± 0.112 ms | 3.778 ± 0.329 ms | **1.548 ± 0.027 ms** |
| large/circleci | 6.908 ± 0.238 ms | 5.634 ± 0.132 ms | **2.005 ± 0.069 ms** |
| massive/data-warehouse | 51.157 ± 0.416 ms | 11.858 ± 0.077 ms | **4.469 ± 0.179 ms** |
| massive/k8s-deployment | 11.749 ± 0.076 ms | 247.397 ± 64.026 ms † | **2.556 ± 0.076 ms** |
| medium/user-profile | 2.835 ± 0.197 ms | 3.827 ± 0.357 ms | **1.507 ± 0.028 ms** |
| small/config | 2.811 ± 0.062 ms | 3.593 ± 0.051 ms | **1.510 ± 0.021 ms** |
| small/prettierrc | 3.029 ± 0.195 ms | 3.700 ± 0.073 ms | **1.529 ± 0.035 ms** |

### validate (invalid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.89 MiB | 10.02 ± 0.01 MiB | **3.98 ± 0.02 MiB** |
| generated/large-array | **55.77 ± 0.01 MiB** | 64.84 MiB | 68.25 ± 0.02 MiB |
| generated/many-defs | 19.09 MiB | 21.38 ± 0.02 MiB | **14.30 MiB** |
| generated/wide-object | 15.09 ± 0.82 MiB | 15.42 MiB | **7.00 MiB** |
| large/api-catalog | 5.91 MiB | 10.50 ± 0.01 MiB | **3.08 MiB** |
| large/circleci | 9.19 ± 0.06 MiB | 13.06 ± 0.02 MiB | **4.25 MiB** |
| massive/data-warehouse | 22.67 ± 0.02 MiB | 21.06 ± 0.01 MiB | **7.41 ± 0.04 MiB** |
| massive/k8s-deployment | 11.77 MiB | 20.55 ± 0.01 MiB | **6.20 MiB** |
| medium/user-profile | 5.70 ± 0.08 MiB | 10.22 MiB | **3.00 MiB** |
| small/config | 5.69 ± 0.02 MiB | 10.00 ± 0.00 MiB | **2.98 ± 0.00 MiB** |
| small/prettierrc | 5.88 MiB | 9.16 ± 0.02 MiB | **3.12 MiB** |

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
