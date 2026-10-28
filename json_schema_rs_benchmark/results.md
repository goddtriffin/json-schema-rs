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
| generated/deep-nesting | **6.183 ± 0.010 ms** | 9.635 ± 0.052 ms |
| generated/large-array | **73.254 ± 0.178 µs** | 1.027 ± 0.002 ms |
| generated/many-defs | **37.130 ± 0.254 ms** | 122.645 ± 2.607 ms |
| generated/wide-object | **138.492 ± 0.184 ms** | 146.991 ± 0.507 ms |
| large/api-catalog | **206.058 ± 0.208 µs** | 3.090 ± 0.005 ms |
| large/circleci | 1.382 ± 0.005 ms | — |
| massive/data-warehouse | **9.382 ± 0.015 ms** | 268.678 ± 0.952 ms |
| massive/k8s-deployment | 18.738 ± 0.982 ms | **1.822 ± 0.002 ms** |
| medium/user-profile | **92.659 ± 0.492 µs** | 1.422 ± 0.002 ms |
| small/config | **29.599 ± 0.353 µs** | 510.239 ± 4.545 µs |

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
| generated/deep-nesting | 1.028 ± 0.019 ms | 206.382 ± 0.640 µs | **64.607 ± 0.072 µs** |
| generated/large-array | 45.047 ± 0.228 µs | 20.879 ± 0.036 µs | **4.331 ± 0.018 µs** |
| generated/many-defs | 13.737 ± 0.054 ms | 3.018 ± 0.007 ms | **707.284 ± 10.414 µs** |
| generated/wide-object | 23.252 ± 0.283 ms | 2.528 ± 0.018 ms | **760.817 ± 1.951 µs** |
| large/api-catalog | 178.427 ± 0.262 µs | 104.034 ± 0.095 µs | **15.661 ± 0.022 µs** |
| large/circleci | 2.852 ± 0.021 ms | 1.011 ± 0.001 ms | **255.006 ± 0.467 µs** |
| massive/data-warehouse | 44.246 ± 0.537 ms | 6.299 ± 0.021 ms | **1.273 ± 0.002 ms** |
| massive/k8s-deployment | 7.499 ± 0.148 ms | 1.611 ± 0.016 ms | **612.736 ± 1.583 µs** |
| medium/user-profile | 67.341 ± 0.186 µs | 40.174 ± 0.162 µs | **5.668 ± 0.016 µs** |
| small/config | 26.976 ± 0.036 µs | 19.144 ± 0.156 µs | **1.828 ± 0.005 µs** |
| small/prettierrc | 204.454 ± 0.485 µs | 110.557 ± 0.107 µs | **25.364 ± 0.045 µs** |

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
| generated/deep-nesting | 6.815 ± 0.015 µs | **2.437 ± 0.020 µs** | 140.358 ± 0.178 µs |
| generated/large-array | 17.966 ± 0.286 ms | **7.242 ± 0.094 ms** | 146.319 ± 0.819 ms |
| generated/many-defs | 1.752 ± 0.030 ms | **618.471 ± 0.789 µs** | 18.801 ± 0.068 ms |
| generated/wide-object | 171.062 ± 0.611 µs | **112.486 ± 0.642 µs** | 787.788 ± 2.116 µs |
| large/api-catalog | 573.726 ± 7.693 ns | **241.105 ± 0.625 ns** | 6.723 ± 0.019 µs |
| large/circleci | 2.174 ± 0.011 µs | **471.203 ± 1.642 ns** | 2.194 ± 0.005 µs |
| massive/data-warehouse | 69.455 ± 0.239 µs | **23.691 ± 0.067 µs** | 610.295 ± 5.509 µs |
| massive/k8s-deployment | 517.661 ± 1.426 ns | **214.963 ± 0.410 ns** | 4.648 ± 0.009 µs |
| medium/user-profile | 839.433 ± 1.498 ns | **369.789 ± 2.458 ns** | 5.847 ± 0.031 µs |
| small/config | 236.674 ± 4.906 ns | **113.369 ± 4.261 ns** | 2.770 ± 0.006 µs |
| small/prettierrc | 720.206 ± 5.103 ns | **185.268 ± 9.232 ns** | 3.150 ± 0.087 µs |

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
| generated/deep-nesting | 7.731 ± 0.045 µs | **2.822 ± 0.041 µs** | 140.083 ± 0.205 µs |
| generated/large-array | 17.940 ± 0.067 ms | **7.309 ± 0.166 ms** | 147.385 ± 1.449 ms |
| generated/many-defs | 1.768 ± 0.008 ms | **613.719 ± 2.035 µs** | 18.814 ± 0.053 ms |
| generated/wide-object | 171.105 ± 0.630 µs | **113.580 ± 1.298 µs** | 790.507 ± 3.602 µs |
| large/api-catalog | 661.245 ± 16.601 ns | **390.718 ± 1.632 ns** | 5.350 ± 0.017 µs |
| large/circleci | **125.730 ± 0.180 ns** | 135.079 ± 4.134 ns | 443.586 ± 27.766 ns |
| massive/data-warehouse | 190.403 ± 5.372 µs | **140.593 ± 4.759 µs** | 889.660 ± 2.893 µs |
| massive/k8s-deployment | 176.929 ± 1.150 ns | **137.606 ± 1.255 ns** | 568.034 ± 3.302 ns |
| medium/user-profile | 372.383 ± 1.947 ns | **307.366 ± 4.453 ns** | 2.373 ± 0.010 µs |
| small/config | 291.548 ± 2.320 ns | **249.490 ± 3.116 ns** | 2.788 ± 0.008 µs |
| small/prettierrc | **533.906 ± 8.035 ns** | 906.978 ± 1.444 ns | 913.177 ± 8.265 ns |

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
| generated/deep-nesting | **11.471 ± 0.216 ms** | 109.249 ± 1.683 ms |
| generated/large-array | **2.676 ± 0.245 ms** | 99.801 ± 2.048 ms |
| generated/many-defs | **47.198 ± 1.135 ms** | 449.586 ± 6.261 ms |
| generated/wide-object | **144.942 ± 3.614 ms** | 510.659 ± 4.358 ms |
| large/api-catalog | **2.247 ± 0.068 ms** | 95.517 ± 1.708 ms |
| large/circleci | 4.237 ± 0.088 ms | — |
| massive/data-warehouse | **14.321 ± 0.576 ms** | 794.004 ± 7.570 ms |
| massive/k8s-deployment | **28.393 ± 0.919 ms** | 87.279 ± 1.516 ms |
| medium/user-profile | **1.861 ± 0.048 ms** | 90.154 ± 3.987 ms |
| small/config | **1.814 ± 0.062 ms** | 86.061 ± 0.764 ms |

### codegen — peak RSS

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | 45.38 ± 0.01 MiB | **37.97 ± 0.01 MiB** |
| generated/large-array | **3.80 ± 0.01 MiB** | 33.31 MiB |
| generated/many-defs | 116.42 MiB | **98.20 ± 0.05 MiB** |
| generated/wide-object | **38.66 ± 0.00 MiB** | 115.67 ± 0.36 MiB |
| large/api-catalog | **4.83 ± 0.02 MiB** | 34.62 MiB |
| large/circleci | 12.66 MiB | — |
| massive/data-warehouse | **37.53 MiB** | 176.41 ± 0.22 MiB |
| massive/k8s-deployment | 96.27 ± 0.14 MiB | **31.83 ± 0.02 MiB** |
| medium/user-profile | **3.84 ± 0.01 MiB** | 33.53 ± 0.02 MiB |
| small/config | **3.34 MiB** | 32.97 MiB |

### validate (valid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.277 ± 0.396 ms | 4.130 ± 0.095 ms | **1.959 ± 0.055 ms** |
| generated/large-array | 60.479 ± 0.729 ms | **45.553 ± 0.682 ms** | 191.823 ± 3.042 ms |
| generated/many-defs | 23.256 ± 1.153 ms | **12.467 ± 0.779 ms** | 28.257 ± 0.651 ms |
| generated/wide-object | 28.054 ± 0.342 ms | 8.211 ± 0.105 ms | **4.146 ± 0.048 ms** |
| large/api-catalog | 3.207 ± 0.269 ms | 4.009 ± 0.101 ms | **1.653 ± 0.033 ms** |
| large/circleci | 6.996 ± 0.090 ms | 5.756 ± 0.081 ms | **2.141 ± 0.028 ms** |
| massive/data-warehouse | 50.081 ± 0.583 ms | 12.006 ± 0.086 ms | **4.337 ± 0.100 ms** |
| massive/k8s-deployment | 11.930 ± 0.103 ms | 508.196 ± 177.887 ms † | **2.752 ± 0.048 ms** |
| medium/user-profile | 2.935 ± 0.136 ms | 3.847 ± 0.108 ms | **1.571 ± 0.037 ms** |
| small/config | 2.883 ± 0.163 ms | 3.686 ± 0.115 ms | **1.558 ± 0.064 ms** |
| small/prettierrc | 3.110 ± 0.138 ms | 3.785 ± 0.104 ms | **1.604 ± 0.030 ms** |

### validate (valid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.89 MiB | 9.91 ± 0.00 MiB | **3.94 ± 0.01 MiB** |
| generated/large-array | **55.77 MiB** | 64.67 ± 0.02 MiB | 68.17 ± 0.01 MiB |
| generated/many-defs | 20.73 MiB | 21.55 ± 0.07 MiB | **14.27 ± 0.03 MiB** |
| generated/wide-object | 15.52 ± 0.17 MiB | 15.30 ± 0.02 MiB | **6.95 MiB** |
| large/api-catalog | 5.86 ± 0.01 MiB | 10.33 ± 0.02 MiB | **3.06 ± 0.01 MiB** |
| large/circleci | 9.14 ± 0.04 MiB | 13.09 ± 0.01 MiB | **4.27 MiB** |
| massive/data-warehouse | 21.97 ± 0.03 MiB | 20.95 ± 0.01 MiB | **7.58 MiB** |
| massive/k8s-deployment | 11.78 MiB | 20.61 ± 0.01 MiB | **6.22 ± 0.01 MiB** |
| medium/user-profile | 5.72 ± 0.01 MiB | 10.08 ± 0.01 MiB | **2.98 ± 0.01 MiB** |
| small/config | 5.72 ± 0.01 MiB | 9.86 ± 0.01 MiB | **2.94 MiB** |
| small/prettierrc | 5.88 ± 0.01 MiB | 8.97 MiB | **3.14 MiB** |

### validate (invalid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.425 ± 0.236 ms | 4.118 ± 0.064 ms | **1.945 ± 0.027 ms** |
| generated/large-array | 60.183 ± 0.616 ms | **46.685 ± 0.561 ms** | 190.603 ± 2.022 ms |
| generated/many-defs | 23.477 ± 1.288 ms | **11.663 ± 0.514 ms** | 28.288 ± 0.642 ms |
| generated/wide-object | 28.139 ± 0.844 ms | 8.273 ± 0.448 ms | **4.237 ± 0.226 ms** |
| large/api-catalog | 3.299 ± 0.328 ms | 3.984 ± 0.050 ms | **1.669 ± 0.049 ms** |
| large/circleci | 6.959 ± 0.248 ms | 5.942 ± 0.497 ms | **2.083 ± 0.062 ms** |
| massive/data-warehouse | 51.136 ± 0.373 ms | 13.725 ± 1.602 ms † | **4.876 ± 0.298 ms** |
| massive/k8s-deployment | 11.946 ± 0.118 ms | 502.572 ± 170.279 ms † | **2.664 ± 0.034 ms** |
| medium/user-profile | 2.927 ± 0.046 ms | 3.809 ± 0.111 ms | **1.555 ± 0.039 ms** |
| small/config | 2.982 ± 0.144 ms | 3.667 ± 0.062 ms | **1.518 ± 0.032 ms** |
| small/prettierrc | 3.157 ± 0.035 ms | 3.752 ± 0.062 ms | **1.565 ± 0.023 ms** |

### validate (invalid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.89 MiB | 10.02 ± 0.01 MiB | **3.97 ± 0.02 MiB** |
| generated/large-array | **55.77 ± 0.01 MiB** | 64.78 ± 0.01 MiB | 68.22 MiB |
| generated/many-defs | 20.72 MiB | 21.44 MiB | **14.30 ± 0.04 MiB** |
| generated/wide-object | 16.78 ± 0.86 MiB | 15.41 ± 0.01 MiB | **6.98 ± 0.01 MiB** |
| large/api-catalog | 5.88 ± 0.02 MiB | 10.50 MiB | **3.08 MiB** |
| large/circleci | 9.12 ± 0.02 MiB | 13.02 ± 0.03 MiB | **4.23 ± 0.01 MiB** |
| massive/data-warehouse | 22.78 MiB | 21.66 ± 0.27 MiB | **7.41 MiB** |
| massive/k8s-deployment | 11.73 MiB | 20.59 ± 0.05 MiB | **6.22 ± 0.00 MiB** |
| medium/user-profile | 5.70 MiB | 10.23 MiB | **3.00 MiB** |
| small/config | 5.72 MiB | 10.02 MiB | **2.98 ± 0.01 MiB** |
| small/prettierrc | 5.88 ± 0.01 MiB | 9.12 ± 0.01 MiB | **3.11 MiB** |

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
