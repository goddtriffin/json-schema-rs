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
| generated/deep-nesting | **6.234 ± 0.018 ms** | 9.639 ± 0.039 ms |
| generated/large-array | **73.283 ± 0.065 µs** | 1.027 ± 0.003 ms |
| generated/many-defs | **37.131 ± 0.284 ms** | 122.579 ± 2.481 ms |
| generated/wide-object | **136.893 ± 1.392 ms** | 147.243 ± 0.702 ms |
| large/api-catalog | **206.955 ± 0.218 µs** | 3.078 ± 0.003 ms |
| large/circleci | 1.392 ± 0.005 ms | — |
| massive/data-warehouse | **9.454 ± 0.005 ms** | 269.913 ± 0.803 ms |
| massive/k8s-deployment | 18.594 ± 1.004 ms | **1.812 ± 0.002 ms** |
| medium/user-profile | **92.851 ± 0.495 µs** | 1.414 ± 0.002 ms |
| small/config | **29.703 ± 0.049 µs** | 512.257 ± 0.799 µs |

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
| generated/deep-nesting | 1.020 ± 0.004 ms | 206.386 ± 0.351 µs | **64.693 ± 0.100 µs** |
| generated/large-array | 44.875 ± 0.107 µs | 20.877 ± 0.032 µs | **4.333 ± 0.011 µs** |
| generated/many-defs | 13.663 ± 0.021 ms | 3.008 ± 0.004 ms | **710.562 ± 4.697 µs** |
| generated/wide-object | 23.158 ± 0.018 ms | 2.506 ± 0.005 ms | **768.875 ± 1.282 µs** |
| large/api-catalog | 177.331 ± 0.417 µs | 103.751 ± 0.134 µs | **15.649 ± 0.018 µs** |
| large/circleci | 2.866 ± 0.014 ms | 1.015 ± 0.005 ms | **254.689 ± 0.243 µs** |
| massive/data-warehouse | 44.293 ± 0.111 ms | 6.330 ± 0.029 ms | **1.263 ± 0.001 ms** |
| massive/k8s-deployment | 7.365 ± 0.010 ms | 1.597 ± 0.001 ms | **606.402 ± 0.789 µs** |
| medium/user-profile | 66.958 ± 0.138 µs | 40.182 ± 0.068 µs | **5.695 ± 0.014 µs** |
| small/config | 26.982 ± 0.100 µs | 19.145 ± 0.182 µs | **1.846 ± 0.010 µs** |
| small/prettierrc | 204.493 ± 0.229 µs | 111.302 ± 0.119 µs | **25.432 ± 0.031 µs** |

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
| generated/deep-nesting | 6.851 ± 0.042 µs | **2.443 ± 0.009 µs** | 139.402 ± 0.236 µs |
| generated/large-array | 18.621 ± 0.735 ms | **7.469 ± 0.102 ms** | 147.217 ± 1.414 ms |
| generated/many-defs | 1.731 ± 0.004 ms | **627.505 ± 1.432 µs** | 19.194 ± 0.157 ms |
| generated/wide-object | 170.998 ± 0.573 µs | **110.161 ± 2.764 µs** | 801.095 ± 4.175 µs |
| large/api-catalog | 562.505 ± 1.373 ns | **242.004 ± 2.861 ns** | 6.727 ± 0.026 µs |
| large/circleci | 2.208 ± 0.034 µs | **465.481 ± 0.875 ns** | 2.188 ± 0.007 µs |
| massive/data-warehouse | 69.696 ± 0.180 µs | **23.778 ± 0.053 µs** | 610.823 ± 4.252 µs |
| massive/k8s-deployment | 520.607 ± 0.863 ns | **212.998 ± 0.674 ns** | 4.633 ± 0.032 µs |
| medium/user-profile | 847.510 ± 3.247 ns | **371.597 ± 1.928 ns** | 5.895 ± 0.028 µs |
| small/config | 225.780 ± 0.301 ns | **112.870 ± 0.390 ns** | 2.776 ± 0.006 µs |
| small/prettierrc | 730.069 ± 3.328 ns | **172.393 ± 0.405 ns** | 3.085 ± 0.008 µs |

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
| generated/deep-nesting | 7.765 ± 0.008 µs | **2.780 ± 0.008 µs** | 139.807 ± 0.152 µs |
| generated/large-array | 18.084 ± 0.030 ms | **7.372 ± 0.015 ms** | 147.749 ± 2.688 ms |
| generated/many-defs | 1.725 ± 0.008 ms | **623.125 ± 0.818 µs** | 18.884 ± 0.129 ms |
| generated/wide-object | 168.362 ± 0.590 µs | **109.133 ± 0.682 µs** | 790.520 ± 1.725 µs |
| large/api-catalog | 658.134 ± 2.691 ns | **390.646 ± 0.901 ns** | 5.321 ± 0.035 µs |
| large/circleci | **128.030 ± 0.412 ns** | 132.559 ± 0.433 ns | 412.223 ± 0.549 ns |
| massive/data-warehouse | 179.717 ± 0.122 µs | **135.545 ± 0.542 µs** | 886.020 ± 3.003 µs |
| massive/k8s-deployment | 180.270 ± 0.737 ns | **134.789 ± 0.371 ns** | 575.022 ± 3.386 ns |
| medium/user-profile | 366.492 ± 1.633 ns | **307.595 ± 2.266 ns** | 2.395 ± 0.009 µs |
| small/config | 285.128 ± 0.351 ns | **244.191 ± 0.768 ns** | 2.767 ± 0.007 µs |
| small/prettierrc | **526.841 ± 5.164 ns** | 900.321 ± 1.535 ns | 885.435 ± 6.167 ns |

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
| generated/deep-nesting | **11.178 ± 0.252 ms** | 108.696 ± 2.120 ms |
| generated/large-array | **1.881 ± 0.056 ms** | 87.715 ± 0.953 ms |
| generated/many-defs | **44.692 ± 0.244 ms** | 444.310 ± 3.362 ms |
| generated/wide-object | **144.220 ± 4.478 ms** | 514.317 ± 3.134 ms |
| large/api-catalog | **2.233 ± 0.049 ms** | 94.417 ± 0.600 ms |
| large/circleci | 4.250 ± 0.127 ms | — |
| massive/data-warehouse | **14.090 ± 0.406 ms** | 787.767 ± 4.854 ms |
| massive/k8s-deployment | **27.978 ± 0.408 ms** | 86.416 ± 0.694 ms |
| medium/user-profile | **1.873 ± 0.040 ms** | 88.687 ± 0.731 ms |
| small/config | **1.710 ± 0.036 ms** | 86.254 ± 0.841 ms |

### codegen — peak RSS

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | 45.44 MiB | **38.58 ± 0.31 MiB** |
| generated/large-array | **3.84 MiB** | 33.31 ± 0.01 MiB |
| generated/many-defs | 115.12 ± 0.01 MiB | **98.09 ± 0.04 MiB** |
| generated/wide-object | **38.78 ± 0.01 MiB** | 115.22 ± 0.06 MiB |
| large/api-catalog | **4.95 ± 0.01 MiB** | 34.66 MiB |
| large/circleci | 12.77 MiB | — |
| massive/data-warehouse | **37.59 MiB** | 177.17 ± 0.16 MiB |
| massive/k8s-deployment | 95.84 ± 0.03 MiB | **31.81 MiB** |
| medium/user-profile | **3.89 ± 0.01 MiB** | 33.59 ± 0.02 MiB |
| small/config | **3.38 ± 0.01 MiB** | 32.94 ± 0.02 MiB |

### validate (valid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.332 ± 0.388 ms | 4.159 ± 0.272 ms | **1.971 ± 0.048 ms** |
| generated/large-array | 58.699 ± 0.266 ms | **44.989 ± 0.709 ms** | 188.482 ± 4.395 ms |
| generated/many-defs | 23.241 ± 0.212 ms | **11.614 ± 0.144 ms** | 26.386 ± 1.350 ms |
| generated/wide-object | 27.838 ± 0.328 ms | 8.111 ± 0.125 ms | **4.144 ± 0.034 ms** |
| large/api-catalog | 3.196 ± 0.057 ms | 4.060 ± 0.067 ms | **1.690 ± 0.054 ms** |
| large/circleci | 7.023 ± 0.419 ms | 5.771 ± 0.115 ms | **2.086 ± 0.031 ms** |
| massive/data-warehouse | 50.020 ± 1.523 ms | 12.009 ± 0.222 ms | **4.339 ± 0.060 ms** |
| massive/k8s-deployment | 12.080 ± 0.771 ms | 351.342 ± 77.137 ms † | **2.734 ± 0.075 ms** |
| medium/user-profile | 2.948 ± 0.077 ms | 3.810 ± 0.156 ms | **1.553 ± 0.032 ms** |
| small/config | 2.912 ± 0.075 ms | 3.768 ± 0.087 ms | **1.618 ± 0.078 ms** |
| small/prettierrc | 3.129 ± 0.332 ms † | 3.810 ± 0.087 ms | **1.698 ± 0.185 ms †** |

### validate (valid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.89 MiB | 9.88 ± 0.02 MiB | **4.08 ± 0.01 MiB** |
| generated/large-array | **55.80 ± 0.02 MiB** | 64.69 ± 0.01 MiB | 68.38 MiB |
| generated/many-defs | 19.09 MiB | 21.36 MiB | **14.38 ± 0.01 MiB** |
| generated/wide-object | 15.11 ± 0.01 MiB | 15.27 ± 0.01 MiB | **7.09 ± 0.02 MiB** |
| large/api-catalog | 5.84 ± 0.02 MiB | 10.34 ± 0.01 MiB | **3.20 ± 0.01 MiB** |
| large/circleci | 9.19 ± 0.03 MiB | 13.12 ± 0.02 MiB | **4.34 ± 0.01 MiB** |
| massive/data-warehouse | 22.05 ± 0.08 MiB | 20.94 MiB | **7.70 ± 0.01 MiB** |
| massive/k8s-deployment | 11.84 ± 0.77 MiB | 20.50 ± 0.03 MiB | **6.28 ± 0.01 MiB** |
| medium/user-profile | 5.70 ± 0.00 MiB | 10.06 ± 0.01 MiB | **3.12 MiB** |
| small/config | 5.67 ± 0.01 MiB | 9.86 ± 0.01 MiB | **3.05 MiB** |
| small/prettierrc | 5.80 ± 0.07 MiB | 8.95 MiB | **3.19 MiB** |

### validate (invalid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.190 ± 0.126 ms | 4.091 ± 0.066 ms | **2.009 ± 0.113 ms** |
| generated/large-array | 58.711 ± 0.425 ms | **44.806 ± 0.351 ms** | 188.001 ± 6.544 ms |
| generated/many-defs | 23.028 ± 0.249 ms | **11.757 ± 0.846 ms** | 26.437 ± 0.304 ms |
| generated/wide-object | 27.820 ± 0.106 ms | 8.121 ± 0.082 ms | **4.132 ± 0.100 ms** |
| large/api-catalog | 3.176 ± 0.040 ms | 4.034 ± 0.083 ms | **1.677 ± 0.030 ms** |
| large/circleci | 6.921 ± 0.044 ms | 5.781 ± 0.221 ms | **2.078 ± 0.030 ms** |
| massive/data-warehouse | 51.277 ± 0.276 ms | 11.993 ± 0.164 ms | **4.583 ± 0.203 ms** |
| massive/k8s-deployment | 11.948 ± 0.075 ms | 324.211 ± 79.551 ms † | **2.772 ± 0.143 ms** |
| medium/user-profile | 2.924 ± 0.045 ms | 3.783 ± 0.073 ms | **1.530 ± 0.020 ms** |
| small/config | 2.877 ± 0.051 ms | 3.839 ± 0.100 ms | **1.531 ± 0.033 ms** |
| small/prettierrc | 3.138 ± 0.058 ms | 3.823 ± 0.090 ms | **1.596 ± 0.030 ms** |

### validate (invalid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.88 ± 0.02 MiB | 10.03 ± 0.02 MiB | **4.12 ± 0.01 MiB** |
| generated/large-array | **55.80 ± 0.02 MiB** | 64.88 MiB | 68.42 ± 0.01 MiB |
| generated/many-defs | 19.09 ± 0.02 MiB | 21.42 MiB | **14.45 MiB** |
| generated/wide-object | 15.11 ± 0.02 MiB | 15.42 ± 0.02 MiB | **7.12 ± 0.01 MiB** |
| large/api-catalog | 5.86 ± 0.02 MiB | 10.48 ± 0.02 MiB | **3.22 ± 0.01 MiB** |
| large/circleci | 9.19 ± 0.04 MiB | 13.11 MiB | **4.36 MiB** |
| massive/data-warehouse | 22.59 ± 0.04 MiB | 21.05 ± 0.02 MiB | **7.47 ± 0.01 MiB** |
| massive/k8s-deployment | 11.84 ± 0.02 MiB | 20.52 ± 0.02 MiB | **6.33 MiB** |
| medium/user-profile | 5.70 ± 0.01 MiB | 10.23 ± 0.01 MiB | **3.14 MiB** |
| small/config | 5.67 ± 0.01 MiB | 10.00 ± 0.01 MiB | **3.11 MiB** |
| small/prettierrc | 5.83 ± 0.01 MiB | 9.14 ± 0.01 MiB | **3.17 MiB** |

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
