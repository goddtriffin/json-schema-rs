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
| in-process | validate (valid) | peak heap | jsonschema | 8 / 11 |
| in-process | validate (invalid) | time | jsonschema | 9 / 11 |
| in-process | validate (invalid) | peak heap | jsonschema | 9 / 11 |
| CLI | codegen | time | jsonschemars | 9 / 9 |
| CLI | codegen | peak RSS | jsonschemars | 8 / 9 |
| CLI | validate (valid) | time | jsonschemars | 10 / 11 |
| CLI | validate (valid) | peak RSS | jsonschemars | 10 / 11 |
| CLI | validate (invalid) | time | jsonschemars | 10 / 11 |
| CLI | validate (invalid) | peak RSS | jsonschemars | 10 / 11 |

## In-process (library calls)

### codegen — time

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | **4.409 ± 0.055 ms** | 9.839 ± 0.202 ms |
| generated/large-array | **63.255 ± 0.929 µs** | 1.042 ± 0.019 ms |
| generated/many-defs | **26.961 ± 0.089 ms** | 124.139 ± 3.019 ms |
| generated/wide-object | **138.573 ± 1.066 ms** | 150.163 ± 3.485 ms |
| large/api-catalog | **196.091 ± 1.643 µs** | 3.098 ± 0.006 ms |
| large/circleci | 1.635 ± 0.012 ms | — |
| massive/data-warehouse | **7.963 ± 0.213 ms** | 269.129 ± 5.162 ms |
| massive/k8s-deployment | 14.924 ± 0.408 ms | **1.837 ± 0.029 ms** |
| medium/user-profile | **82.786 ± 3.311 µs** | 1.454 ± 0.076 ms |
| small/config | **27.185 ± 0.048 µs** | 511.164 ± 0.705 µs |

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
| generated/deep-nesting | 1.026 ± 0.020 ms | 206.359 ± 0.381 µs | **112.946 ± 0.115 µs** |
| generated/large-array | 44.577 ± 0.186 µs | 20.750 ± 0.030 µs | **11.912 ± 0.035 µs** |
| generated/many-defs | 13.636 ± 0.019 ms | 2.998 ± 0.004 ms | **1.723 ± 0.031 ms** |
| generated/wide-object | 23.961 ± 0.890 ms | 2.491 ± 0.039 ms | **1.560 ± 0.003 ms** |
| large/api-catalog | 177.715 ± 0.513 µs | 102.634 ± 0.169 µs | **46.573 ± 0.078 µs** |
| large/circleci | 2.860 ± 0.002 ms | 1.024 ± 0.005 ms | **568.131 ± 0.623 µs** |
| massive/data-warehouse | 44.535 ± 0.150 ms | 6.260 ± 0.005 ms | **3.021 ± 0.004 ms** |
| massive/k8s-deployment | 7.384 ± 0.102 ms | 1.595 ± 0.026 ms | **887.517 ± 13.109 µs** |
| medium/user-profile | 68.278 ± 1.958 µs | 39.827 ± 0.350 µs | **17.801 ± 0.047 µs** |
| small/config | 26.743 ± 0.053 µs | 18.998 ± 0.154 µs | **7.721 ± 0.127 µs** |
| small/prettierrc | 204.432 ± 1.117 µs | 110.860 ± 0.831 µs | **63.721 ± 0.123 µs** |

### schema compile — peak heap

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 636.04 KiB | 534.41 KiB | **159.04 KiB** |
| generated/large-array | 66.67 KiB | 34.10 KiB | **21.05 KiB** |
| generated/many-defs | 8.10 MiB | 3.88 MiB | **2.67 MiB** |
| generated/wide-object | 13.73 MiB | 4.03 MiB | **2.89 MiB** |
| large/api-catalog | 266.71 KiB | 151.24 KiB | **69.65 KiB** |
| large/circleci | 3.85 MiB | 1.75 MiB | **904.01 KiB** |
| massive/data-warehouse | 16.73 MiB | 9.42 MiB | **4.63 MiB** |
| massive/k8s-deployment | 7.45 MiB | 3.28 MiB | **1.54 MiB** |
| medium/user-profile | 121.82 KiB | 59.13 KiB | **29.42 KiB** |
| small/config | 38.35 KiB | 24.08 KiB | **10.94 KiB** |
| small/prettierrc | 476.85 KiB | 261.04 KiB | **116.26 KiB** |

### validate (valid) — time

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.377 ± 0.031 µs | **2.437 ± 0.011 µs** | 4.026 ± 0.068 µs |
| generated/large-array | 18.043 ± 0.198 ms | **7.237 ± 0.247 ms** | 14.711 ± 0.358 ms |
| generated/many-defs | 1.717 ± 0.022 ms | **621.773 ± 2.605 µs** | 972.991 ± 19.088 µs |
| generated/wide-object | 169.807 ± 1.080 µs | **109.521 ± 1.468 µs** | 221.293 ± 1.744 µs |
| large/api-catalog | 561.713 ± 0.765 ns | **242.472 ± 3.269 ns** | 708.354 ± 9.336 ns |
| large/circleci | 2.177 ± 0.008 µs | **466.849 ± 7.285 ns** | 1.147 ± 0.004 µs |
| massive/data-warehouse | 69.737 ± 1.009 µs | **23.964 ± 0.400 µs** | 90.520 ± 0.936 µs |
| massive/k8s-deployment | 520.615 ± 5.004 ns | **214.340 ± 2.496 ns** | 529.678 ± 11.426 ns |
| medium/user-profile | 857.630 ± 10.123 ns | **374.816 ± 6.135 ns** | 1.003 ± 0.018 µs |
| small/config | 225.048 ± 1.882 ns | **112.775 ± 2.367 ns** | 283.106 ± 4.256 ns |
| small/prettierrc | 716.189 ± 4.337 ns | **173.094 ± 1.650 ns** | 456.572 ± 3.228 ns |

### validate (valid) — peak heap

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 2.25 KiB | 0.03 KiB | **0.00 KiB** |
| generated/large-array | 0.19 KiB | **0.03 KiB** | 0.08 KiB |
| generated/many-defs | 0.56 KiB | **0.03 KiB** | 0.38 KiB |
| generated/wide-object | 0.19 KiB | 0.03 KiB | **0.02 KiB** |
| large/api-catalog | 0.19 KiB | **0.03 KiB** | 0.26 KiB |
| large/circleci | 2.58 KiB | **0.03 KiB** | 0.28 KiB |
| massive/data-warehouse | 0.19 KiB | **0.03 KiB** | 0.26 KiB |
| massive/k8s-deployment | 0.19 KiB | 0.03 KiB | **0.02 KiB** |
| medium/user-profile | 0.19 KiB | **0.03 KiB** | 0.26 KiB |
| small/config | 0.19 KiB | **0.03 KiB** | 0.04 KiB |
| small/prettierrc | 0.77 KiB | **0.03 KiB** | 0.08 KiB |

### validate (invalid) — time

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 7.741 ± 0.079 µs | **2.794 ± 0.019 µs** | 6.445 ± 0.074 µs |
| generated/large-array | 18.917 ± 0.345 ms | **7.492 ± 0.423 ms** | 15.674 ± 0.752 ms |
| generated/many-defs | 1.719 ± 0.007 ms | **620.267 ± 4.451 µs** | 981.088 ± 14.280 µs |
| generated/wide-object | 168.006 ± 0.312 µs | **111.265 ± 2.595 µs** | 212.122 ± 0.640 µs |
| large/api-catalog | 651.732 ± 4.664 ns | **398.650 ± 5.426 ns** | 1.274 ± 0.006 µs |
| large/circleci | 137.458 ± 1.471 ns | **128.146 ± 1.968 ns** | 309.764 ± 4.354 ns |
| massive/data-warehouse | 182.640 ± 1.775 µs | 138.384 ± 0.898 µs | **115.666 ± 0.943 µs** |
| massive/k8s-deployment | 180.771 ± 0.453 ns | **133.384 ± 3.411 ns** | 290.386 ± 5.922 ns |
| medium/user-profile | 370.272 ± 5.358 ns | **309.955 ± 3.889 ns** | 694.351 ± 7.535 ns |
| small/config | 289.369 ± 0.491 ns | **247.760 ± 5.680 ns** | 713.196 ± 12.535 ns |
| small/prettierrc | 529.145 ± 5.869 ns | 937.804 ± 12.591 ns | **142.098 ± 0.421 ns** |

### validate (invalid) — peak heap

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 2.97 KiB | **0.75 KiB** | 3.79 KiB |
| generated/large-array | 1.37 KiB | **0.46 KiB** | 0.62 KiB |
| generated/many-defs | 7.77 KiB | **0.77 KiB** | 1.20 KiB |
| generated/wide-object | 0.74 KiB | **0.41 KiB** | 0.52 KiB |
| large/api-catalog | 1.44 KiB | **0.83 KiB** | 1.94 KiB |
| large/circleci | 0.74 KiB | **0.40 KiB** | 0.94 KiB |
| massive/data-warehouse | 467.89 KiB | 111.78 KiB | **0.62 KiB** |
| massive/k8s-deployment | 0.81 KiB | **0.48 KiB** | 0.55 KiB |
| medium/user-profile | 1.30 KiB | **0.73 KiB** | 0.83 KiB |
| small/config | 1.30 KiB | **0.71 KiB** | 1.53 KiB |
| small/prettierrc | 2.38 KiB | 2.69 KiB | **0.47 KiB** |

## CLI (end to end)

### codegen — time

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | **9.017 ± 0.433 ms** | 109.220 ± 2.292 ms |
| generated/large-array | **1.854 ± 0.059 ms** | 87.518 ± 1.166 ms |
| generated/many-defs | **32.950 ± 0.561 ms** | 442.618 ± 6.981 ms |
| generated/wide-object | **143.437 ± 2.134 ms** | 516.723 ± 7.081 ms |
| large/api-catalog | **2.171 ± 0.074 ms** | 94.016 ± 3.232 ms |
| large/circleci | 4.347 ± 0.097 ms | — |
| massive/data-warehouse | **12.751 ± 0.373 ms** | 795.460 ± 7.210 ms |
| massive/k8s-deployment | **22.567 ± 0.671 ms** | 88.469 ± 3.329 ms |
| medium/user-profile | **1.831 ± 0.054 ms** | 87.281 ± 2.153 ms |
| small/config | **1.753 ± 0.097 ms** | 84.483 ± 1.317 ms |

### codegen — peak RSS

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | **24.31 MiB** | 38.00 ± 0.01 MiB |
| generated/large-array | **3.97 ± 0.01 MiB** | 33.33 ± 0.02 MiB |
| generated/many-defs | **57.30 ± 0.01 MiB** | 98.16 ± 0.05 MiB |
| generated/wide-object | **25.22 ± 0.18 MiB** | 115.20 ± 0.60 MiB |
| large/api-catalog | **4.81 MiB** | 34.67 ± 0.01 MiB |
| large/circleci | 10.88 ± 0.00 MiB | — |
| massive/data-warehouse | **32.94 MiB** | 176.77 ± 0.36 MiB |
| massive/k8s-deployment | 57.98 ± 0.10 MiB | **31.77 MiB** |
| medium/user-profile | **4.05 MiB** | 33.58 ± 0.02 MiB |
| small/config | **3.73 MiB** | 32.91 ± 0.02 MiB |

### validate (valid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.171 ± 0.374 ms | 4.170 ± 0.280 ms | **1.749 ± 0.029 ms** |
| generated/large-array | 59.975 ± 1.280 ms | **45.231 ± 0.974 ms** | 50.620 ± 0.431 ms |
| generated/many-defs | 22.941 ± 0.293 ms | 11.579 ± 0.395 ms | **7.265 ± 0.093 ms** |
| generated/wide-object | 28.227 ± 0.533 ms | 8.025 ± 0.122 ms | **4.182 ± 0.038 ms** |
| large/api-catalog | 3.052 ± 0.105 ms | 3.872 ± 0.176 ms | **1.789 ± 0.121 ms** |
| large/circleci | 6.919 ± 0.197 ms | 5.834 ± 0.170 ms | **2.514 ± 0.042 ms** |
| massive/data-warehouse | 50.532 ± 1.162 ms | 11.845 ± 0.476 ms | **5.781 ± 0.076 ms** |
| massive/k8s-deployment | 11.722 ± 0.098 ms | 425.453 ± 83.643 ms † | **2.978 ± 0.096 ms** |
| medium/user-profile | 2.835 ± 0.259 ms | 3.761 ± 0.078 ms | **1.619 ± 0.057 ms** |
| small/config | 2.785 ± 0.083 ms | 3.564 ± 0.216 ms | **1.617 ± 0.076 ms** |
| small/prettierrc | 3.630 ± 0.507 ms † | 3.677 ± 0.099 ms | **1.677 ± 0.053 ms** |

### validate (valid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.81 MiB | 9.89 MiB | **4.02 ± 0.01 MiB** |
| generated/large-array | **55.81 ± 0.01 MiB** | 64.69 MiB | 59.47 ± 0.01 MiB |
| generated/many-defs | 20.77 MiB | 21.36 MiB | **12.88 ± 0.02 MiB** |
| generated/wide-object | 15.11 ± 0.01 MiB | 15.30 ± 0.01 MiB | **7.61 ± 0.01 MiB** |
| large/api-catalog | 5.89 ± 0.03 MiB | 10.36 ± 0.00 MiB | **3.73 ± 0.01 MiB** |
| large/circleci | 9.16 MiB | 13.05 ± 0.00 MiB | **5.31 MiB** |
| massive/data-warehouse | 23.72 ± 0.86 MiB | 20.94 ± 0.01 MiB | **10.16 MiB** |
| massive/k8s-deployment | 11.86 MiB | 20.53 ± 0.04 MiB | **6.06 ± 0.02 MiB** |
| medium/user-profile | 5.75 MiB | 10.09 MiB | **3.59 MiB** |
| small/config | 5.75 ± 0.02 MiB | 9.91 MiB | **3.53 MiB** |
| small/prettierrc | 5.83 ± 0.01 MiB | 8.94 ± 0.01 MiB | **3.69 ± 0.00 MiB** |

### validate (invalid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.132 ± 0.157 ms | 4.065 ± 0.082 ms | **1.770 ± 0.054 ms** |
| generated/large-array | 59.365 ± 1.526 ms | **44.876 ± 0.664 ms** | 50.985 ± 0.610 ms |
| generated/many-defs | 23.304 ± 0.738 ms | 11.677 ± 0.358 ms | **7.245 ± 0.226 ms** |
| generated/wide-object | 27.800 ± 0.829 ms | 8.229 ± 0.306 ms | **4.272 ± 0.234 ms** |
| large/api-catalog | 3.157 ± 0.174 ms | 3.874 ± 0.058 ms | **1.712 ± 0.057 ms** |
| large/circleci | 6.935 ± 0.220 ms | 5.803 ± 0.192 ms | **2.757 ± 0.172 ms** |
| massive/data-warehouse | 52.045 ± 1.143 ms | 12.857 ± 0.484 ms | **5.655 ± 0.056 ms** |
| massive/k8s-deployment | 11.672 ± 0.169 ms | 360.614 ± 79.295 ms † | **2.957 ± 0.162 ms** |
| medium/user-profile | 2.874 ± 0.117 ms | 3.671 ± 0.061 ms | **1.579 ± 0.029 ms** |
| small/config | 2.805 ± 0.127 ms | 3.640 ± 0.065 ms | **1.547 ± 0.024 ms** |
| small/prettierrc | 3.099 ± 0.191 ms | 3.633 ± 0.064 ms | **1.660 ± 0.032 ms** |

### validate (invalid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.80 ± 0.01 MiB | 10.03 ± 0.01 MiB | **4.11 MiB** |
| generated/large-array | **55.80 ± 0.01 MiB** | 64.81 ± 0.01 MiB | 59.56 MiB |
| generated/many-defs | 20.80 MiB | 21.44 ± 0.01 MiB | **12.95 MiB** |
| generated/wide-object | 16.81 ± 0.60 MiB | 15.41 MiB | **7.72 MiB** |
| large/api-catalog | 5.89 ± 0.03 MiB | 10.48 MiB | **3.78 ± 0.01 MiB** |
| large/circleci | 9.06 MiB | 13.03 ± 0.03 MiB | **5.36 ± 0.01 MiB** |
| massive/data-warehouse | 23.58 ± 0.25 MiB | 21.58 ± 0.24 MiB | **9.94 MiB** |
| massive/k8s-deployment | 11.84 MiB | 20.62 ± 0.03 MiB | **6.08 ± 0.01 MiB** |
| medium/user-profile | 5.70 ± 0.01 MiB | 10.19 ± 0.01 MiB | **3.64 MiB** |
| small/config | 5.78 MiB | 10.02 ± 0.01 MiB | **3.64 MiB** |
| small/prettierrc | 5.83 ± 0.01 MiB | 9.12 MiB | **3.70 MiB** |

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
