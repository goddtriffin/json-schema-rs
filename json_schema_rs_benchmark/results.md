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
| generated/deep-nesting | **4.427 ± 0.015 ms** | 9.626 ± 0.028 ms |
| generated/large-array | **63.253 ± 0.225 µs** | 1.029 ± 0.003 ms |
| generated/many-defs | **27.190 ± 0.104 ms** | 121.094 ± 0.486 ms |
| generated/wide-object | **137.547 ± 0.636 ms** | 145.813 ± 0.193 ms |
| large/api-catalog | **195.767 ± 0.190 µs** | 3.089 ± 0.004 ms |
| large/circleci | 1.635 ± 0.003 ms | — |
| massive/data-warehouse | **7.802 ± 0.041 ms** | 266.523 ± 0.779 ms |
| massive/k8s-deployment | 14.659 ± 0.046 ms | **1.818 ± 0.006 ms** |
| medium/user-profile | **81.434 ± 0.162 µs** | 1.415 ± 0.008 ms |
| small/config | **27.297 ± 0.051 µs** | 509.564 ± 1.203 µs |

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
| generated/deep-nesting | 1.031 ± 0.015 ms | 204.930 ± 0.718 µs | **69.592 ± 0.173 µs** |
| generated/large-array | 44.786 ± 0.096 µs | 20.611 ± 0.050 µs | **5.990 ± 0.018 µs** |
| generated/many-defs | 13.740 ± 0.016 ms | 2.995 ± 0.011 ms | **1.037 ± 0.021 ms** |
| generated/wide-object | 23.327 ± 0.054 ms | 2.501 ± 0.004 ms | **801.997 ± 0.698 µs** |
| large/api-catalog | 178.706 ± 0.292 µs | 103.138 ± 0.167 µs | **23.608 ± 0.062 µs** |
| large/circleci | 2.854 ± 0.013 ms | 1.016 ± 0.003 ms | **301.921 ± 0.301 µs** |
| massive/data-warehouse | 44.640 ± 0.501 ms | 6.272 ± 0.019 ms | **1.542 ± 0.002 ms** |
| massive/k8s-deployment | 7.391 ± 0.106 ms | 1.585 ± 0.004 ms | **489.585 ± 0.786 µs** |
| medium/user-profile | 67.130 ± 0.152 µs | 39.911 ± 0.144 µs | **9.008 ± 0.021 µs** |
| small/config | 27.118 ± 0.036 µs | 18.883 ± 0.054 µs | **3.132 ± 0.008 µs** |
| small/prettierrc | 203.689 ± 0.196 µs | 111.021 ± 0.073 µs | **33.042 ± 0.059 µs** |

### schema compile — peak heap

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 636.04 KiB | 534.41 KiB | **118.03 KiB** |
| generated/large-array | 66.67 KiB | 34.10 KiB | **17.26 KiB** |
| generated/many-defs | 8.10 MiB | 3.88 MiB | **2.30 MiB** |
| generated/wide-object | 13.73 MiB | 4.03 MiB | **2.18 MiB** |
| large/api-catalog | 266.71 KiB | 151.24 KiB | **52.19 KiB** |
| large/circleci | 3.85 MiB | 1.75 MiB | **737.98 KiB** |
| massive/data-warehouse | 16.73 MiB | 9.42 MiB | **3.37 MiB** |
| massive/k8s-deployment | 7.45 MiB | 3.28 MiB | **1.22 MiB** |
| medium/user-profile | 121.82 KiB | 59.13 KiB | **22.40 KiB** |
| small/config | 38.35 KiB | 24.08 KiB | **8.30 KiB** |
| small/prettierrc | 476.85 KiB | 261.04 KiB | **97.81 KiB** |

### validate (valid) — time

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.337 ± 0.012 µs | **2.365 ± 0.033 µs** | 138.507 ± 0.315 µs |
| generated/large-array | 17.891 ± 0.051 ms | **7.242 ± 0.032 ms** | 145.145 ± 1.005 ms |
| generated/many-defs | 1.752 ± 0.010 ms | **616.950 ± 1.103 µs** | 18.616 ± 0.057 ms |
| generated/wide-object | 170.464 ± 0.742 µs | **108.970 ± 1.171 µs** | 658.262 ± 5.280 µs |
| large/api-catalog | 564.219 ± 1.714 ns | **241.411 ± 1.374 ns** | 6.687 ± 0.017 µs |
| large/circleci | 2.168 ± 0.010 µs | **461.189 ± 0.998 ns** | 2.060 ± 0.005 µs |
| massive/data-warehouse | 69.248 ± 0.221 µs | **23.640 ± 0.034 µs** | 620.603 ± 2.644 µs |
| massive/k8s-deployment | 518.179 ± 1.673 ns | **209.299 ± 0.416 ns** | 4.631 ± 0.012 µs |
| medium/user-profile | 834.412 ± 1.330 ns | **372.135 ± 2.168 ns** | 5.908 ± 0.023 µs |
| small/config | 223.006 ± 0.616 ns | **112.065 ± 0.149 ns** | 2.807 ± 0.014 µs |
| small/prettierrc | 721.826 ± 6.825 ns | **175.570 ± 1.101 ns** | 2.945 ± 0.030 µs |

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
| generated/deep-nesting | 7.459 ± 0.013 µs | **2.834 ± 0.007 µs** | 139.088 ± 0.131 µs |
| generated/large-array | 17.928 ± 0.037 ms | **7.204 ± 0.025 ms** | 144.027 ± 0.176 ms |
| generated/many-defs | 1.765 ± 0.011 ms | **615.075 ± 1.078 µs** | 18.633 ± 0.071 ms |
| generated/wide-object | 170.421 ± 0.515 µs | **109.387 ± 0.416 µs** | 656.924 ± 1.387 µs |
| large/api-catalog | 654.624 ± 1.112 ns | **388.955 ± 0.895 ns** | 5.384 ± 0.013 µs |
| large/circleci | 136.212 ± 0.226 ns | 133.412 ± 1.458 ns | 414.133 ± 0.526 ns |
| massive/data-warehouse | 178.418 ± 0.697 µs | **135.106 ± 0.408 µs** | 910.887 ± 2.613 µs |
| massive/k8s-deployment | 183.699 ± 0.287 ns | **134.061 ± 0.602 ns** | 581.950 ± 1.625 ns |
| medium/user-profile | 367.627 ± 3.646 ns | **316.829 ± 1.604 ns** | 2.411 ± 0.007 µs |
| small/config | 287.671 ± 0.760 ns | **244.182 ± 3.110 ns** | 2.838 ± 0.027 µs |
| small/prettierrc | **522.879 ± 1.234 ns** | 890.982 ± 2.398 ns | 913.488 ± 2.241 ns |

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
| generated/deep-nesting | **8.361 ± 0.442 ms** | 107.373 ± 0.619 ms |
| generated/large-array | **1.779 ± 0.028 ms** | 86.608 ± 0.368 ms |
| generated/many-defs | **32.550 ± 0.164 ms** | 434.449 ± 1.686 ms |
| generated/wide-object | **139.036 ± 8.740 ms** | 505.476 ± 24.285 ms |
| large/api-catalog | **2.121 ± 0.066 ms** | 93.475 ± 0.392 ms |
| large/circleci | 4.203 ± 0.061 ms | — |
| massive/data-warehouse | **12.189 ± 0.134 ms** | 784.422 ± 3.520 ms |
| massive/k8s-deployment | **22.052 ± 0.224 ms** | 85.512 ± 0.479 ms |
| medium/user-profile | **1.853 ± 0.024 ms** | 86.814 ± 0.619 ms |
| small/config | **1.736 ± 0.055 ms** | 84.261 ± 1.113 ms |

### codegen — peak RSS

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | **24.20 MiB** | 38.00 ± 0.01 MiB |
| generated/large-array | **3.83 MiB** | 33.33 MiB |
| generated/many-defs | **57.12 ± 0.01 MiB** | 98.02 ± 0.01 MiB |
| generated/wide-object | **25.00 MiB** | 114.53 ± 0.21 MiB |
| large/api-catalog | **4.67 ± 0.01 MiB** | 34.67 ± 0.02 MiB |
| large/circleci | 10.69 ± 0.02 MiB | — |
| massive/data-warehouse | **32.78 ± 0.02 MiB** | 175.83 ± 0.02 MiB |
| massive/k8s-deployment | 57.86 ± 0.02 MiB | **31.77 ± 0.01 MiB** |
| medium/user-profile | **3.88 ± 0.01 MiB** | 33.56 MiB |
| small/config | **3.64 ± 0.01 MiB** | 32.92 ± 0.01 MiB |

### validate (valid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.292 ± 0.352 ms | 3.939 ± 0.034 ms | **1.871 ± 0.028 ms** |
| generated/large-array | 58.883 ± 0.467 ms | **44.704 ± 0.421 ms** | 185.758 ± 1.841 ms |
| generated/many-defs | 22.805 ± 0.273 ms | **11.342 ± 0.354 ms** | 25.531 ± 0.330 ms |
| generated/wide-object | 27.747 ± 0.094 ms | 8.001 ± 0.151 ms | **3.881 ± 0.033 ms** |
| large/api-catalog | 3.057 ± 0.063 ms | 3.808 ± 0.200 ms | **1.619 ± 0.032 ms** |
| large/circleci | 6.733 ± 0.061 ms | 5.537 ± 0.063 ms | **2.043 ± 0.035 ms** |
| massive/data-warehouse | 49.868 ± 0.323 ms | 11.925 ± 0.127 ms | **4.666 ± 0.048 ms** |
| massive/k8s-deployment | 11.675 ± 0.139 ms | 256.590 ± 60.562 ms † | **2.407 ± 0.043 ms** |
| medium/user-profile | 2.825 ± 0.032 ms | 3.569 ± 0.043 ms | **1.581 ± 0.036 ms** |
| small/config | 2.772 ± 0.036 ms | 3.507 ± 0.039 ms | **1.533 ± 0.027 ms** |
| small/prettierrc | 3.045 ± 0.054 ms | 3.554 ± 0.037 ms | **1.612 ± 0.032 ms** |

### validate (valid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.81 MiB | 9.91 ± 0.01 MiB | **4.00 ± 0.00 MiB** |
| generated/large-array | **55.80 MiB** | 64.69 ± 0.02 MiB | 67.38 ± 0.01 MiB |
| generated/many-defs | 19.11 ± 0.01 MiB | 21.34 ± 0.01 MiB | **12.48 ± 0.02 MiB** |
| generated/wide-object | 15.19 MiB | 15.30 MiB | **6.69 ± 0.01 MiB** |
| large/api-catalog | 5.88 MiB | 10.36 ± 0.01 MiB | **3.45 MiB** |
| large/circleci | 9.08 ± 0.01 MiB | 13.09 MiB | **4.50 ± 0.01 MiB** |
| massive/data-warehouse | 21.95 MiB | 20.98 MiB | **8.42 ± 0.01 MiB** |
| massive/k8s-deployment | 11.89 ± 0.06 MiB | 20.62 MiB | **5.27 ± 0.01 MiB** |
| medium/user-profile | 5.73 MiB | 10.06 MiB | **3.41 MiB** |
| small/config | 5.75 MiB | 9.88 MiB | **3.33 ± 0.01 MiB** |
| small/prettierrc | 5.83 MiB | 8.95 ± 0.01 MiB | **3.41 MiB** |

### validate (invalid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 3.998 ± 0.040 ms | 3.941 ± 0.068 ms | **1.886 ± 0.039 ms** |
| generated/large-array | 58.559 ± 0.287 ms | **45.461 ± 1.086 ms** | 185.708 ± 1.727 ms |
| generated/many-defs | 22.848 ± 0.205 ms | **11.226 ± 0.072 ms** | 25.689 ± 0.283 ms |
| generated/wide-object | 27.826 ± 0.086 ms | 7.940 ± 0.207 ms | **3.865 ± 0.031 ms** |
| large/api-catalog | 3.058 ± 0.031 ms | 3.801 ± 0.056 ms | **1.599 ± 0.028 ms** |
| large/circleci | 6.771 ± 0.075 ms | 5.654 ± 0.068 ms | **2.038 ± 0.034 ms** |
| massive/data-warehouse | 50.866 ± 0.228 ms | 11.891 ± 0.161 ms | **4.875 ± 0.468 ms** |
| massive/k8s-deployment | 11.697 ± 0.052 ms | 264.455 ± 72.722 ms † | **2.364 ± 0.023 ms** |
| medium/user-profile | 2.831 ± 0.030 ms | 3.579 ± 0.051 ms | **1.555 ± 0.034 ms** |
| small/config | 2.783 ± 0.073 ms | 3.525 ± 0.044 ms | **1.544 ± 0.035 ms** |
| small/prettierrc | 3.038 ± 0.044 ms | 3.800 ± 0.683 ms † | **1.560 ± 0.056 ms** |

### validate (invalid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.81 MiB | 10.00 ± 0.02 MiB | **4.05 MiB** |
| generated/large-array | **55.80 ± 0.01 MiB** | 64.83 ± 0.02 MiB | 67.44 ± 0.01 MiB |
| generated/many-defs | 19.11 MiB | 21.45 ± 0.00 MiB | **12.55 ± 0.01 MiB** |
| generated/wide-object | 15.11 ± 0.01 MiB | 15.38 ± 0.01 MiB | **6.73 MiB** |
| large/api-catalog | 5.88 ± 0.01 MiB | 10.50 MiB | **3.45 ± 0.01 MiB** |
| large/circleci | 9.12 MiB | 13.03 ± 0.02 MiB | **4.53 MiB** |
| massive/data-warehouse | 22.64 MiB | 21.03 ± 0.02 MiB | **8.19 ± 0.02 MiB** |
| massive/k8s-deployment | 11.86 ± 0.02 MiB | 20.50 ± 0.01 MiB | **5.27 ± 0.01 MiB** |
| medium/user-profile | 5.72 MiB | 10.22 ± 0.00 MiB | **3.38 MiB** |
| small/config | 5.72 MiB | 10.02 ± 0.01 MiB | **3.36 ± 0.01 MiB** |
| small/prettierrc | 5.81 ± 0.01 MiB | 9.14 ± 0.02 MiB | **3.42 ± 0.01 MiB** |

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
