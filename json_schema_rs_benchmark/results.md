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
| CLI | validate (invalid) | time | jsonschemars | 7 / 11 |
| CLI | validate (invalid) | peak RSS | jsonschemars | 10 / 11 |

## In-process (library calls)

### codegen — time

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | **6.510 ± 0.218 ms** | 9.634 ± 0.052 ms |
| generated/large-array | **73.574 ± 0.251 µs** | 1.029 ± 0.002 ms |
| generated/many-defs | **36.436 ± 0.320 ms** | 122.174 ± 0.304 ms |
| generated/wide-object | **138.839 ± 0.948 ms** | 146.891 ± 2.538 ms |
| large/api-catalog | **206.397 ± 0.192 µs** | 3.106 ± 0.005 ms |
| large/circleci | 1.378 ± 0.003 ms | — |
| massive/data-warehouse | **9.373 ± 0.045 ms** | 269.713 ± 0.587 ms |
| massive/k8s-deployment | 18.728 ± 0.072 ms | **1.816 ± 0.004 ms** |
| medium/user-profile | **92.234 ± 0.108 µs** | 1.433 ± 0.020 ms |
| small/config | **29.529 ± 0.038 µs** | 508.678 ± 1.991 µs |

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
| generated/deep-nesting | 1.028 ± 0.015 ms | 207.492 ± 0.537 µs | **64.527 ± 0.135 µs** |
| generated/large-array | 45.060 ± 0.113 µs | 20.847 ± 0.057 µs | **4.347 ± 0.016 µs** |
| generated/many-defs | 13.750 ± 0.051 ms | 3.019 ± 0.008 ms | **704.864 ± 12.887 µs** |
| generated/wide-object | 23.125 ± 0.040 ms | 2.527 ± 0.005 ms | **763.193 ± 0.949 µs** |
| large/api-catalog | 177.919 ± 0.429 µs | 103.841 ± 0.098 µs | **15.612 ± 0.019 µs** |
| large/circleci | 2.866 ± 0.014 ms | 1.016 ± 0.001 ms | **254.742 ± 0.465 µs** |
| massive/data-warehouse | 43.558 ± 0.281 ms | 6.292 ± 0.010 ms | **1.260 ± 0.008 ms** |
| massive/k8s-deployment | 7.352 ± 0.045 ms | 1.615 ± 0.004 ms | **611.512 ± 1.045 µs** |
| medium/user-profile | 66.845 ± 0.357 µs | 40.082 ± 0.056 µs | **5.689 ± 0.011 µs** |
| small/config | 27.012 ± 0.085 µs | 19.009 ± 0.059 µs | **1.842 ± 0.007 µs** |
| small/prettierrc | 204.729 ± 0.245 µs | 110.327 ± 0.225 µs | **25.526 ± 0.031 µs** |

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
| generated/deep-nesting | 6.345 ± 0.020 µs | **2.441 ± 0.008 µs** | 139.564 ± 0.145 µs |
| generated/large-array | 18.058 ± 0.121 ms | **7.385 ± 0.038 ms** | 147.880 ± 0.915 ms |
| generated/many-defs | 1.731 ± 0.007 ms | **616.617 ± 1.044 µs** | 18.781 ± 0.080 ms |
| generated/wide-object | 169.983 ± 0.638 µs | **114.282 ± 1.910 µs** | 788.357 ± 2.379 µs |
| large/api-catalog | 561.737 ± 2.354 ns | **235.020 ± 0.978 ns** | 6.751 ± 0.043 µs |
| large/circleci | 2.207 ± 0.020 µs | **465.454 ± 0.903 ns** | 2.185 ± 0.003 µs |
| massive/data-warehouse | 69.859 ± 0.399 µs | **23.736 ± 0.152 µs** | 607.082 ± 3.497 µs |
| massive/k8s-deployment | 516.942 ± 2.299 ns | **215.400 ± 1.195 ns** | 4.637 ± 0.036 µs |
| medium/user-profile | 839.904 ± 5.194 ns | **377.800 ± 5.243 ns** | 5.854 ± 0.020 µs |
| small/config | 225.514 ± 0.331 ns | **112.669 ± 0.504 ns** | 2.791 ± 0.030 µs |
| small/prettierrc | 719.584 ± 2.183 ns | **176.168 ± 0.962 ns** | 3.087 ± 0.016 µs |

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
| generated/deep-nesting | 7.474 ± 0.023 µs | **2.809 ± 0.004 µs** | 141.474 ± 0.231 µs |
| generated/large-array | 18.012 ± 0.041 ms | **7.345 ± 0.049 ms** | 147.015 ± 0.829 ms |
| generated/many-defs | 1.732 ± 0.014 ms | **620.356 ± 1.965 µs** | 18.803 ± 0.061 ms |
| generated/wide-object | 167.686 ± 1.301 µs | **117.174 ± 2.416 µs** | 791.987 ± 3.956 µs |
| large/api-catalog | 656.930 ± 1.855 ns | **392.948 ± 1.472 ns** | 5.330 ± 0.012 µs |
| large/circleci | **125.414 ± 0.134 ns** | 134.430 ± 0.900 ns | 399.856 ± 1.206 ns |
| massive/data-warehouse | 180.373 ± 0.750 µs | **137.529 ± 0.217 µs** | 887.821 ± 3.890 µs |
| massive/k8s-deployment | 178.627 ± 0.194 ns | **133.953 ± 0.514 ns** | 560.179 ± 2.364 ns |
| medium/user-profile | 371.986 ± 0.709 ns | **304.982 ± 0.669 ns** | 2.372 ± 0.010 µs |
| small/config | 285.326 ± 1.072 ns | **248.074 ± 0.355 ns** | 2.802 ± 0.058 µs |
| small/prettierrc | **533.267 ± 3.406 ns** | 902.793 ± 4.144 ns | 885.443 ± 1.120 ns |

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
| generated/deep-nesting | **12.814 ± 0.599 ms** | 115.578 ± 0.518 ms |
| generated/large-array | **2.565 ± 0.061 ms** | 94.308 ± 0.492 ms |
| generated/many-defs | **46.115 ± 0.369 ms** | 444.786 ± 1.624 ms |
| generated/wide-object | **145.948 ± 1.143 ms** | 513.884 ± 1.083 ms |
| large/api-catalog | **2.936 ± 0.099 ms** | 101.075 ± 0.423 ms |
| large/circleci | 4.725 ± 0.482 ms † | — |
| massive/data-warehouse | **15.656 ± 0.149 ms** | 792.538 ± 1.948 ms |
| massive/k8s-deployment | **29.902 ± 0.157 ms** | 93.303 ± 0.888 ms |
| medium/user-profile | **3.705 ± 0.188 ms** | 95.169 ± 0.438 ms |
| small/config | **2.408 ± 0.074 ms** | 92.119 ± 0.649 ms |

### codegen — peak RSS

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | 45.39 MiB | **38.00 ± 0.01 MiB** |
| generated/large-array | **3.81 ± 0.01 MiB** | 33.30 MiB |
| generated/many-defs | 115.08 MiB | **97.94 ± 0.02 MiB** |
| generated/wide-object | **38.67 MiB** | 114.75 ± 0.30 MiB |
| large/api-catalog | **4.83 MiB** | 34.67 ± 0.01 MiB |
| large/circleci | 12.64 ± 0.00 MiB | — |
| massive/data-warehouse | **36.92 MiB** | 175.86 ± 0.01 MiB |
| massive/k8s-deployment | 95.97 ± 0.03 MiB | **31.78 ± 0.01 MiB** |
| medium/user-profile | **3.84 MiB** | 33.59 ± 0.01 MiB |
| small/config | **3.34 MiB** | 32.92 MiB |

### validate (valid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.992 ± 0.314 ms | 4.627 ± 0.082 ms | **3.169 ± 0.231 ms** |
| generated/large-array | 60.581 ± 0.469 ms | **46.215 ± 0.350 ms** | 189.403 ± 0.812 ms |
| generated/many-defs | 24.980 ± 0.254 ms | **11.865 ± 0.149 ms** | 27.941 ± 0.694 ms |
| generated/wide-object | 29.403 ± 0.153 ms | 8.904 ± 0.415 ms | **6.034 ± 0.101 ms** |
| large/api-catalog | 3.780 ± 0.082 ms | 4.605 ± 0.111 ms | **2.427 ± 0.069 ms** |
| large/circleci | 8.367 ± 0.274 ms | 6.253 ± 0.269 ms | **4.009 ± 0.324 ms** |
| massive/data-warehouse | 52.047 ± 0.675 ms | 13.066 ± 0.262 ms | **6.013 ± 0.498 ms** |
| massive/k8s-deployment | 13.578 ± 0.353 ms | 293.339 ± 45.506 ms † | **4.562 ± 0.094 ms** |
| medium/user-profile | 3.649 ± 0.059 ms | 4.587 ± 0.257 ms | **3.175 ± 0.366 ms †** |
| small/config | 3.495 ± 0.055 ms | 4.271 ± 0.080 ms | **2.582 ± 0.176 ms** |
| small/prettierrc | 3.733 ± 0.091 ms | 4.634 ± 0.336 ms | **3.551 ± 0.089 ms** |

### validate (valid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.89 MiB | 9.91 MiB | **3.91 ± 0.01 MiB** |
| generated/large-array | **55.78 ± 0.02 MiB** | 64.67 MiB | 68.17 MiB |
| generated/many-defs | 19.06 ± 0.29 MiB | 21.38 MiB | **14.27 MiB** |
| generated/wide-object | 15.09 MiB | 15.30 MiB | **6.95 MiB** |
| large/api-catalog | 5.88 ± 0.01 MiB | 10.36 MiB | **3.05 ± 0.01 MiB** |
| large/circleci | 9.11 ± 0.05 MiB | 13.05 MiB | **4.27 MiB** |
| massive/data-warehouse | 21.97 MiB | 20.98 ± 0.02 MiB | **7.56 ± 0.01 MiB** |
| massive/k8s-deployment | 11.78 MiB | 20.47 MiB | **6.20 ± 0.00 MiB** |
| medium/user-profile | 5.70 ± 0.01 MiB | 10.06 MiB | **2.98 MiB** |
| small/config | 5.70 MiB | 9.84 ± 0.00 MiB | **2.94 MiB** |
| small/prettierrc | 5.88 MiB | 8.97 MiB | **3.14 MiB** |

### validate (invalid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 5.039 ± 0.165 ms | 4.564 ± 0.091 ms | **2.793 ± 0.067 ms** |
| generated/large-array | 60.555 ± 0.319 ms | **46.255 ± 0.542 ms** | 189.844 ± 3.973 ms |
| generated/many-defs | 24.707 ± 0.203 ms | **12.130 ± 0.261 ms** | 28.229 ± 0.349 ms |
| generated/wide-object | 29.572 ± 0.152 ms | 8.648 ± 0.115 ms | **6.236 ± 0.126 ms** |
| large/api-catalog | 3.809 ± 0.040 ms | 4.429 ± 0.049 ms | **2.512 ± 0.049 ms** |
| large/circleci | 8.377 ± 0.335 ms | 6.110 ± 0.081 ms | **4.387 ± 0.139 ms** |
| massive/data-warehouse | 53.084 ± 0.328 ms | 12.630 ± 0.279 ms | **6.452 ± 0.082 ms** |
| massive/k8s-deployment | 13.524 ± 0.083 ms | 273.396 ± 38.199 ms † | **4.881 ± 0.329 ms** |
| medium/user-profile | 3.694 ± 0.052 ms | 4.282 ± 0.070 ms | 3.849 ± 0.119 ms |
| small/config | 3.671 ± 0.401 ms † | 4.130 ± 0.081 ms | **2.434 ± 0.047 ms** |
| small/prettierrc | 3.705 ± 0.050 ms | 4.247 ± 0.077 ms | 3.759 ± 0.157 ms |

### validate (invalid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.86 ± 0.01 MiB | 10.03 ± 0.02 MiB | **3.98 MiB** |
| generated/large-array | **55.77 MiB** | 64.80 ± 0.01 MiB | 68.22 ± 0.01 MiB |
| generated/many-defs | 19.11 ± 0.01 MiB | 21.44 ± 0.03 MiB | **14.31 ± 0.01 MiB** |
| generated/wide-object | 15.11 MiB | 15.42 ± 0.12 MiB | **6.98 MiB** |
| large/api-catalog | 5.91 MiB | 10.50 ± 0.01 MiB | **3.08 MiB** |
| large/circleci | 9.08 MiB | 13.12 MiB | **4.23 ± 0.02 MiB** |
| massive/data-warehouse | 22.77 MiB | 21.05 ± 0.01 MiB | **7.33 ± 0.01 MiB** |
| massive/k8s-deployment | 11.78 MiB | 20.52 MiB | **6.22 MiB** |
| medium/user-profile | 5.72 MiB | 10.20 ± 0.01 MiB | **3.00 MiB** |
| small/config | 5.70 ± 0.01 MiB | 10.02 MiB | **2.98 MiB** |
| small/prettierrc | 5.83 ± 0.01 MiB | 9.11 ± 0.01 MiB | **3.12 ± 0.01 MiB** |

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
