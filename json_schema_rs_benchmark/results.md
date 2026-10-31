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
| in-process | schema compile | time | jsonschemars | 12 / 12 |
| in-process | schema compile | peak heap | jsonschemars | 12 / 12 |
| in-process | validate (valid) | time | jsonschema | 11 / 12 |
| in-process | validate (valid) | peak heap | jsonschemars | 11 / 12 |
| in-process | validate (invalid) | time | jsonschema | 9 / 12 |
| in-process | validate (invalid) | peak heap | jsonschema | 11 / 12 |
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
| generated/deep-nesting | **372.544 ± 0.890 µs** | 9.604 ± 0.145 ms |
| generated/large-array | **44.947 ± 0.053 µs** | 1.031 ± 0.012 ms |
| generated/many-defs | **4.317 ± 0.019 ms** | 120.091 ± 1.519 ms |
| generated/wide-object | **5.584 ± 0.080 ms** | 147.885 ± 0.943 ms |
| large/api-catalog | **165.651 ± 2.397 µs** | 3.105 ± 0.064 ms |
| large/circleci | 1.667 ± 0.010 ms | — |
| massive/data-warehouse | **8.494 ± 0.132 ms** | 267.358 ± 2.022 ms |
| massive/k8s-deployment | 4.722 ± 0.028 ms | **1.823 ± 0.013 ms** |
| medium/package-json | 16.239 ± 0.183 ms | — |
| medium/user-profile | **61.376 ± 0.757 µs** | 1.428 ± 0.026 ms |
| small/config | **20.695 ± 0.092 µs** | 508.346 ± 4.633 µs |
| small/prettierrc | 322.971 ± 0.943 µs | — |

### codegen — peak heap

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | **342.50 KiB** | 12.65 MiB |
| generated/large-array | **30.60 KiB** | 1.23 MiB |
| generated/many-defs | **3.28 MiB** | 133.38 MiB |
| generated/wide-object | **4.66 MiB** | 162.36 MiB |
| large/api-catalog | **95.97 KiB** | 3.64 MiB |
| large/circleci | 1.55 MiB | — |
| massive/data-warehouse | **5.65 MiB** | 288.90 MiB |
| massive/k8s-deployment | 3.42 MiB | **2.79 MiB** |
| medium/package-json | 15.63 MiB | — |
| medium/user-profile | **41.17 KiB** | 1.71 MiB |
| small/config | **15.60 KiB** | 639.58 KiB |
| small/prettierrc | 189.71 KiB | — |

### schema compile — time

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 1.021 ± 0.011 ms | 203.246 ± 0.601 µs | **100.838 ± 1.332 µs** |
| generated/large-array | 44.310 ± 0.164 µs | 20.521 ± 0.196 µs | **10.636 ± 0.115 µs** |
| generated/many-defs | 13.769 ± 0.247 ms | 2.994 ± 0.035 ms | **1.618 ± 0.003 ms** |
| generated/wide-object | 23.342 ± 0.211 ms | 2.491 ± 0.003 ms | **1.288 ± 0.015 ms** |
| large/api-catalog | 177.455 ± 0.309 µs | 102.130 ± 0.782 µs | **43.671 ± 0.192 µs** |
| large/circleci | 2.843 ± 0.014 ms | 1.019 ± 0.008 ms | **505.522 ± 1.955 µs** |
| massive/data-warehouse | 43.628 ± 0.501 ms | 6.261 ± 0.050 ms | **2.614 ± 0.029 ms** |
| massive/k8s-deployment | 7.300 ± 0.027 ms | 1.595 ± 0.018 ms | **735.556 ± 0.537 µs** |
| medium/package-json | 68.504 ± 0.639 ms | 14.102 ± 0.244 ms | **5.785 ± 0.004 ms** |
| medium/user-profile | 66.910 ± 0.401 µs | 39.407 ± 0.069 µs | **16.224 ± 0.024 µs** |
| small/config | 27.066 ± 0.370 µs | 19.031 ± 0.648 µs | **7.467 ± 0.037 µs** |
| small/prettierrc | 204.592 ± 1.884 µs | 110.488 ± 1.131 µs | **56.821 ± 0.212 µs** |

### schema compile — peak heap

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 636.04 KiB | 534.41 KiB | **152.81 KiB** |
| generated/large-array | 66.67 KiB | 34.10 KiB | **20.40 KiB** |
| generated/many-defs | 8.10 MiB | 3.88 MiB | **2.64 MiB** |
| generated/wide-object | 13.73 MiB | 4.03 MiB | **2.69 MiB** |
| large/api-catalog | 266.71 KiB | 151.24 KiB | **68.20 KiB** |
| large/circleci | 3.85 MiB | 1.75 MiB | **869.02 KiB** |
| massive/data-warehouse | 16.73 MiB | 9.42 MiB | **4.04 MiB** |
| massive/k8s-deployment | 7.45 MiB | 3.28 MiB | **1.45 MiB** |
| medium/package-json | 29.52 MiB | 21.86 MiB | **11.36 MiB** |
| medium/user-profile | 121.82 KiB | 59.13 KiB | **28.64 KiB** |
| small/config | 38.35 KiB | 24.08 KiB | **10.83 KiB** |
| small/prettierrc | 476.85 KiB | 261.04 KiB | **115.55 KiB** |

### validate (valid) — time

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.680 ± 0.011 µs | **2.439 ± 0.017 µs** | 3.069 ± 0.052 µs |
| generated/large-array | 18.102 ± 0.136 ms | **7.309 ± 0.097 ms** | 8.650 ± 0.136 ms |
| generated/many-defs | 1.778 ± 0.014 ms | **620.822 ± 6.204 µs** | 741.447 ± 2.318 µs |
| generated/wide-object | 174.792 ± 0.955 µs | 109.022 ± 0.584 µs | **54.132 ± 0.663 µs** |
| large/api-catalog | 568.422 ± 15.794 ns | **238.795 ± 1.602 ns** | 417.728 ± 3.038 ns |
| large/circleci | 2.212 ± 0.025 µs | **469.720 ± 4.637 ns** | 1.004 ± 0.005 µs |
| massive/data-warehouse | 70.487 ± 0.637 µs | **24.177 ± 0.115 µs** | 45.273 ± 0.434 µs |
| massive/k8s-deployment | 521.279 ± 2.244 ns | **209.602 ± 1.324 ns** | 499.528 ± 2.715 ns |
| medium/package-json | 535.728 ± 1.572 ns | **227.375 ± 2.177 ns** | 486.204 ± 2.246 ns |
| medium/user-profile | 853.783 ± 8.504 ns | **372.700 ± 1.304 ns** | 490.240 ± 3.609 ns |
| small/config | 228.897 ± 1.487 ns | **112.839 ± 1.124 ns** | 146.696 ± 1.250 ns |
| small/prettierrc | 727.063 ± 2.872 ns | **171.885 ± 1.424 ns** | 279.305 ± 3.559 ns |

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
| medium/package-json | 0.19 KiB | 0.03 KiB | **0.00 KiB** |
| medium/user-profile | 0.19 KiB | 0.03 KiB | **0.00 KiB** |
| small/config | 0.19 KiB | 0.03 KiB | **0.00 KiB** |
| small/prettierrc | 0.77 KiB | 0.03 KiB | **0.00 KiB** |

### validate (invalid) — time

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 7.813 ± 0.056 µs | **2.804 ± 0.010 µs** | 5.156 ± 0.093 µs |
| generated/large-array | 18.133 ± 0.083 ms | **7.423 ± 0.044 ms** | 8.705 ± 0.068 ms |
| generated/many-defs | 1.775 ± 0.009 ms | **627.185 ± 8.723 µs** | 763.414 ± 7.512 µs |
| generated/wide-object | 173.839 ± 1.290 µs | 110.709 ± 1.969 µs | **54.261 ± 0.377 µs** |
| large/api-catalog | 671.697 ± 4.993 ns | **393.739 ± 4.142 ns** | 1.051 ± 0.011 µs |
| large/circleci | **127.223 ± 0.173 ns** | 131.827 ± 2.980 ns | 277.355 ± 5.000 ns |
| massive/data-warehouse | 183.819 ± 1.345 µs | **144.712 ± 3.793 µs** | 273.323 ± 1.666 µs |
| massive/k8s-deployment | 181.696 ± 1.118 ns | **133.986 ± 1.639 ns** | 247.620 ± 3.599 ns |
| medium/package-json | 174.309 ± 2.267 ns | **156.009 ± 1.027 ns** | 245.636 ± 2.399 ns |
| medium/user-profile | 375.789 ± 0.416 ns | **311.053 ± 3.085 ns** | 548.750 ± 8.106 ns |
| small/config | 290.657 ± 4.162 ns | **244.160 ± 0.811 ns** | 541.063 ± 11.656 ns |
| small/prettierrc | **536.775 ± 3.458 ns** | 900.057 ± 5.200 ns | 1.534 ± 0.005 µs |

### validate (invalid) — peak heap

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 2.97 KiB | **0.75 KiB** | 5.25 KiB |
| generated/large-array | 1.37 KiB | **0.46 KiB** | 0.84 KiB |
| generated/many-defs | 7.77 KiB | **0.77 KiB** | 2.25 KiB |
| generated/wide-object | 0.74 KiB | **0.41 KiB** | 0.76 KiB |
| large/api-catalog | 1.44 KiB | **0.83 KiB** | 2.01 KiB |
| large/circleci | 0.74 KiB | **0.40 KiB** | 0.93 KiB |
| massive/data-warehouse | 467.89 KiB | **111.78 KiB** | 247.91 KiB |
| massive/k8s-deployment | 0.81 KiB | **0.48 KiB** | 0.75 KiB |
| medium/package-json | 0.74 KiB | **0.40 KiB** | 0.75 KiB |
| medium/user-profile | 1.30 KiB | **0.73 KiB** | 1.48 KiB |
| small/config | 1.30 KiB | **0.71 KiB** | 1.60 KiB |
| small/prettierrc | **2.38 KiB** | 2.69 KiB | 3.27 KiB |

## CLI (end to end)

### codegen — time

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | **3.036 ± 0.346 ms †** | 110.240 ± 0.851 ms |
| generated/large-array | **1.975 ± 0.054 ms** | 89.061 ± 1.449 ms |
| generated/many-defs | **7.908 ± 0.088 ms** | 443.208 ± 5.683 ms |
| generated/wide-object | **9.562 ± 0.154 ms** | 515.094 ± 4.535 ms |
| large/api-catalog | **2.272 ± 0.060 ms** | 96.390 ± 1.479 ms |
| large/circleci | 4.610 ± 0.173 ms | — |
| massive/data-warehouse | **12.695 ± 0.099 ms** | 790.445 ± 6.405 ms |
| massive/k8s-deployment | **8.552 ± 0.157 ms** | 87.195 ± 0.784 ms |
| medium/package-json | 35.464 ± 0.620 ms | — |
| medium/user-profile | **2.012 ± 0.099 ms** | 90.678 ± 2.332 ms |
| small/config | **1.858 ± 0.056 ms** | 88.001 ± 1.648 ms |
| small/prettierrc | 2.423 ± 0.043 ms | — |

### codegen — peak RSS

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | **4.97 ± 0.02 MiB** | 38.02 MiB |
| generated/large-array | **3.77 ± 0.01 MiB** | 33.30 ± 0.02 MiB |
| generated/many-defs | **8.72 ± 0.01 MiB** | 98.09 ± 0.67 MiB |
| generated/wide-object | **11.48 ± 0.12 MiB** | 114.77 ± 0.10 MiB |
| large/api-catalog | **4.20 ± 0.01 MiB** | 34.64 ± 0.01 MiB |
| large/circleci | 6.67 MiB | — |
| massive/data-warehouse | **11.53 ± 0.02 MiB** | 176.28 ± 0.40 MiB |
| massive/k8s-deployment | **8.86 MiB** | 31.80 ± 0.02 MiB |
| medium/package-json | 19.59 MiB | — |
| medium/user-profile | **3.88 MiB** | 33.53 ± 0.04 MiB |
| small/config | **3.59 ± 0.01 MiB** | 32.91 ± 0.02 MiB |
| small/prettierrc | 4.61 ± 0.01 MiB | — |

### validate (valid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.458 ± 0.499 ms † | 4.449 ± 0.167 ms | **1.950 ± 0.075 ms** |
| generated/large-array | 59.134 ± 0.758 ms | 45.434 ± 0.682 ms | 45.882 ± 0.627 ms |
| generated/many-defs | 23.003 ± 0.322 ms | 11.612 ± 0.565 ms | **7.211 ± 0.220 ms** |
| generated/wide-object | 27.699 ± 0.410 ms | 8.093 ± 0.098 ms | **3.914 ± 0.076 ms** |
| large/api-catalog | 3.321 ± 0.140 ms | 4.203 ± 0.292 ms | **1.827 ± 0.030 ms** |
| large/circleci | 7.190 ± 0.143 ms | 5.993 ± 0.184 ms | **2.765 ± 0.087 ms** |
| massive/data-warehouse | 50.169 ± 0.515 ms | 11.725 ± 0.054 ms | **5.439 ± 0.100 ms** |
| massive/k8s-deployment | 12.263 ± 0.190 ms | 346.751 ± 431.946 ms † | **3.102 ± 0.361 ms †** |
| medium/package-json | 75.131 ± 0.766 ms | — | — |
| medium/user-profile | 2.996 ± 0.072 ms | 3.919 ± 0.132 ms | **1.802 ± 0.046 ms** |
| small/config | 3.064 ± 0.124 ms | 3.797 ± 0.059 ms | **1.772 ± 0.059 ms** |
| small/prettierrc | 3.253 ± 0.160 ms | 3.883 ± 0.333 ms | **1.751 ± 0.049 ms** |

### validate (valid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.97 ± 0.04 MiB | 9.89 ± 0.01 MiB | **3.92 ± 0.01 MiB** |
| generated/large-array | **55.80 MiB** | 64.69 ± 0.01 MiB | 59.48 MiB |
| generated/many-defs | 20.70 MiB | 21.38 ± 0.01 MiB | **12.92 ± 0.01 MiB** |
| generated/wide-object | 16.20 ± 0.50 MiB | 15.27 ± 0.01 MiB | **7.20 MiB** |
| large/api-catalog | 5.89 ± 0.01 MiB | 10.34 ± 0.01 MiB | **3.77 ± 0.01 MiB** |
| large/circleci | 9.08 MiB | 13.28 MiB | **5.33 MiB** |
| massive/data-warehouse | 22.05 MiB | 21.00 ± 0.04 MiB | **9.34 MiB** |
| massive/k8s-deployment | 11.80 MiB | 20.73 MiB | **6.00 ± 0.01 MiB** |
| medium/package-json | 27.67 ± 0.02 MiB | — | — |
| medium/user-profile | 5.70 ± 0.01 MiB | 10.08 MiB | **3.62 ± 0.01 MiB** |
| small/config | 5.72 ± 0.01 MiB | 9.86 MiB | **3.55 MiB** |
| small/prettierrc | 5.83 ± 0.02 MiB | 8.94 ± 0.03 MiB | **3.72 MiB** |

### validate (invalid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.316 ± 0.120 ms | 4.120 ± 0.038 ms | **1.918 ± 0.048 ms** |
| generated/large-array | 58.654 ± 0.991 ms | 45.763 ± 1.300 ms | 45.770 ± 0.988 ms |
| generated/many-defs | 22.851 ± 0.225 ms | 11.779 ± 0.210 ms | **7.145 ± 0.107 ms** |
| generated/wide-object | 27.732 ± 0.136 ms | 8.102 ± 0.112 ms | **4.081 ± 0.332 ms** |
| large/api-catalog | 3.280 ± 0.073 ms | 4.165 ± 0.139 ms | **1.865 ± 0.109 ms** |
| large/circleci | 7.127 ± 0.223 ms | 6.208 ± 0.111 ms | **2.763 ± 0.080 ms** |
| massive/data-warehouse | 51.143 ± 0.316 ms | 11.972 ± 0.220 ms | **5.535 ± 0.145 ms** |
| massive/k8s-deployment | 11.928 ± 0.139 ms | 285.931 ± 61.905 ms † | **2.992 ± 0.122 ms** |
| medium/package-json | 74.917 ± 0.996 ms | — | — |
| medium/user-profile | 3.128 ± 0.145 ms | 3.926 ± 0.193 ms | **1.729 ± 0.046 ms** |
| small/config | 3.069 ± 0.197 ms | 3.913 ± 0.167 ms | **1.684 ± 0.051 ms** |
| small/prettierrc | 3.327 ± 0.116 ms | 3.841 ± 0.087 ms | **1.796 ± 0.053 ms** |

### validate (invalid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.98 MiB | 10.02 ± 0.02 MiB | **4.03 ± 0.01 MiB** |
| generated/large-array | **55.81 MiB** | 64.84 MiB | 59.53 ± 0.01 MiB |
| generated/many-defs | 19.11 ± 0.01 MiB | 21.44 MiB | **12.98 MiB** |
| generated/wide-object | 15.09 ± 0.01 MiB | 15.39 MiB | **7.28 MiB** |
| large/api-catalog | 5.91 ± 0.01 MiB | 10.48 MiB | **3.80 ± 0.01 MiB** |
| large/circleci | 9.81 ± 0.33 MiB | 13.03 ± 0.00 MiB | **5.34 ± 0.02 MiB** |
| massive/data-warehouse | 22.52 MiB | 21.05 ± 0.02 MiB | **9.52 ± 0.01 MiB** |
| massive/k8s-deployment | 11.81 ± 0.01 MiB | 20.55 MiB | **6.00 ± 0.01 MiB** |
| medium/package-json | 27.67 ± 0.01 MiB | — | — |
| medium/user-profile | 5.75 MiB | 10.22 ± 0.01 MiB | **3.67 MiB** |
| small/config | 5.75 MiB | 10.02 ± 0.01 MiB | **3.64 MiB** |
| small/prettierrc | 6.09 MiB | 9.16 ± 0.01 MiB | **3.80 MiB** |

## Compatibility

218 of 228 attempted cells pass. Rows with at least one failure:

| Tier | Operation | Fixture | boon | jsonschema | jsonschema-cli | jsonschemars | typify |
|---|---|---|---|---|---|---|---|
| in-process | codegen | large/circleci | — | — | — | pass | FAIL: panic: not implemented: if/then/else schemas are not supported: { "if": { "required": [ "type" ], "properties": { "type": { "const": "release" } } }, "then": { "required": [ "plan_name" ], "properties": { "plan_name": { "type": "string", "markdownDescription": "Required plan name for release jobs." } }, "additionalProperties": true, "markdownDescription": "https://circleci.com/docs/reference/configuration-reference/#job-type\n\nRelease job." }, "else": { "if": { "required": [ "type" ], "properties": { "type": { "enum": [ "lock", "unlock" ] } } }, "then": { "required": [ "key" ], "properties": { "key": { "type": "string", "markdownDescription": "Required key for lock/unlock jobs." } }, "additionalProperties": true, "markdownDescription": "https://circleci.com/docs/reference/configuration-reference/#job-type\n\nA lock/unlock job" }, "else": { "if": { "required": [ "type" ], "properties": { "type": { "enum": [ "approval", "no-op" ] } } }, "then": { "additionalProperties": true, "markdownDescription": "https://circleci.com/docs/reference/configuration-reference/#job-type\n\nApproval/no-op jobs primarily act as workflow markers. Other keys like `steps:` are permitted but will be ignored in the final pipeline." }, "else": { "if": { "anyOf": [ { "not": { "required": [ "type" ] } }, { "required": [ "type" ], "properties": { "type": { "const": "build" } } } ], "markdownDescription": "https://circleci.com/docs/reference/configuration-reference/#job-type\n\nA build job. The job explicitly has type 'build' or defaults to build when no type specified." }, "then": { "anyOf": [ { "type": "object", "required": [ "executor" ], "markdownDescription": "A job that uses an executor defined elsewhere in the configuration." }, { "oneOf": [ { "type": "object", "required": [ "machine" ] }, { "type": "object", "required": [ "docker" ] }, { "type": "object", "required": [ "macos" ] } ] } ], "required": [ "steps" ], "properties": { "circleci_ip_ranges": { "type": "boolean", "markdownDescription": "Enable running jobs with a set of well-defined IP address ranges." }, "description": { "type": "string" }, "docker": { "type": "array", "items": { "type": "object", "required": [ "image" ], "properties": { "auth": { "type": "object", "required": [ "password", "username" ], "properties": { "password": { "type": "string" }, "username": { "type": "string" } }, "additionalProperties": false }, "aws_auth": { "oneOf": [ { "type": "object", "required": [ "aws_access_key_id", "aws_secret_access_key" ], "properties": { "aws_access_key_id": { "type": "string" }, "aws_secret_access_key": { "type": "string" } }, "additionalProperties": false }, { "type": "object", "required": [ "oidc_role_arn" ], "properties": { "oidc_role_arn": { "type": "string" } }, "additionalProperties": false } ] }, "command": { "oneOf": [ { "type": "string" }, { "type": "array", "items": { "type": "string" } } ] }, "entrypoint": { "oneOf": [ { "type": "string" }, { "type": "array", "items": { "type": "string" } } ] }, "environment": { "oneOf": [ { "type": [ "string", "null" ], "markdownDescription": "A map of environment variable names and values." }, { "type": "array", "items": { "$ref": "#/definitions/environment" }, "markdownDescription": "An array of strings in the form KEY=VALUE" }, { "type": "object", "additionalProperties": { "type": [ "string", "boolean", "number", "null" ] }, "markdownDescription": "The value should be a string, but we allow null, boolean and numbers too. Examples: `FOO: true` and `BAR: 2` `BAZ:`" } ] }, "image": { "type": "string" }, "name": { "type": "string" }, "user": { "type": "string" } }, "additionalProperties": false }, "minItems": 1 }, "environment": { "oneOf": [ { "type": [ "string", "null" ], "markdownDescription": "A map of environment variable names and values." }, { "type": "array", "items": { "$ref": "#/definitions/environment" }, "markdownDescription": "An array of strings in the form KEY=VALUE" }, { "type": "object", "additionalProperties": { "type": [ "string", "boolean", "number", "null" ] }, "markdownDescription": "The value should be a string, but we allow null, boolean and numbers too. Examples: `FOO: true` and `BAR: 2` `BAZ:`" } ] }, "executor": { "oneOf": [ { "type": "string", "markdownDescription": "https://circleci.com/docs/reference/reusing-config/#executor\n\nThe name of the executor to use (defined via the top level executors map or from an orb)." }, { "type": "object", "minProperties": 1, "required": [ "name" ], "properties": { "name": { "type": "string" } }, "additionalProperties": true, "markdownDescription": "Executor stanza to use for the job, with optional parameter overrides." } ] }, "machine": { "oneOf": [ { "anyOf": [ { "type": "string", "pattern": " *<< *parameters.([^ ]+) *>> *" }, { "type": "boolean" }, { "type": "number" }, { "type": "string" } ] }, { "type": "object", "properties": { "docker_layer_caching": { "anyOf": [ { "type": "string", "pattern": " *<< *parameters.([^ ]+) *>> *" }, { "type": "boolean" }, { "type": "number" }, { "type": "string" } ], "markdownDescription": "Set to `true` to enable [Docker Layer Caching](https://circleci.com/docs/docker-layer-caching). Note: If you haven't already, you must open a support ticket to have a CircleCI Sales representative contact you about enabling this feature on your account for an additional fee." }, "enabled": { "anyOf": [ { "type": "string", "pattern": " *<< *parameters.([^ ]+) *>> *" }, { "type": "boolean" }, { "type": "number" }, { "type": "string" } ], "markdownDescription": "A boolean or a template parameter evaluating to a boolean" }, "image": { "type": "string" }, "resource_class": { "type": "string" }, "shell": { "type": "string" } }, "additionalProperties": false } ], "markdownDescription": "Use the default machine executor image" }, "macos": { "type": "object", "required": [ "xcode" ], "properties": { "resource_class": { "type": "string" }, "shell": { "type": "string" }, "xcode": { "type": [ "string", "number" ] } }, "additionalProperties": false }, "parallelism": { "anyOf": [ { "oneOf": [ { "type": "string", "pattern": " *<< *parameters.([^ ]+) *>> *" }, { "type": "string", "pattern": " *<< *pipeline.parameters.([^ ]+) *>> *" } ] }, { "type": "integer" } ], "markdownDescription": "A integer or a parameter evaluating to a integer" }, "parameters": { "type": "object", "minProperties": 1, "additionalProperties": { "type": "object", "required": [ "type" ], "properties": { "default": { "oneOf": [ { "type": "string" }, { "type": "boolean" }, { "type": "integer" }, { "type": "array", "items": { "$ref": "#/definitions/step" } } ] }, "description": { "type": "string", "markdownDescription": "A description of the parameter" }, "enum": { "type": "array", "items": { "type": "string" }, "minItems": 1, "markdownDescription": "List of allowed values for an `enum` type parameter" }, "type": { "type": "string", "enum": [ "boolean", "string", "steps", "enum", "executor", "integer", "env_var_name" ], "markdownDescription": "The parameter type. See [Parameter Types](https://circleci.com/docs/reusing-config#parameter-syntax) for details:\n- `boolean`: A boolean parameter\n- `string`: A string parameter \n- `integer`: An integer parameter\n- `enum`: An enum parameter (list of allowed string values)\n- `executor`: An executor parameter (allows job to choose executor)\n- `steps`: A steps parameter (allows mixing predefined and user-defined steps)\n- `env_var_name`: An environment variable name parameter (must match POSIX_NAME regexp)" } }, "additionalProperties": false }, "markdownDescription": "https://circleci.com/docs/reusing-config#using-the-parameters-declaration\n\nA map of parameter keys." }, "resource_class": { "type": "string" }, "retention": { "type": "object", "properties": { "caches": { "type": "string", "pattern": "^([1-9]\|1[0-5])d$" } }, "additionalProperties": false }, "shell": { "oneOf": [ { "type": "string" }, { "type": "array", "items": { "type": "string" } } ] }, "steps": { "type": "array", "items": { "$ref": "#/definitions/step" }, "minItems": 1 }, "type": { "const": "build", "markdownDescription": "The job type. If not specified, defaults to build." }, "working_directory": { "type": "string", "markdownDescription": "In which directory to run the steps. (default: `~/project`. `project` is a literal string, not the name of the project.) You can also refer the directory with `$CIRCLE_WORKING_DIRECTORY` environment variable." } }, "additionalProperties": false, "markdownDescription": "Validation: The job must adhere to the structure of a build job." }, "else": false } } } } |
| in-process | codegen | medium/package-json | — | — | — | pass | FAIL: cannot load the fixture's resource documents |
| in-process | codegen | small/prettierrc | — | — | — | pass | FAIL: codegen error: value does not conform to the given schema |
| CLI | codegen | large/circleci | — | — | — | pass | FAIL: exit 101: not implemented: if/then/else schemas are not supported: { |
| CLI | codegen | medium/package-json | — | — | — | pass | FAIL: cannot load the fixture's resource documents |
| CLI | codegen | small/prettierrc | — | — | — | pass | FAIL: exit 1: value does not conform to the given schema |
| CLI | validate (valid) | medium/package-json | pass | — | FAIL: cannot load the fixture's resource documents | FAIL: cannot load the fixture's resource documents | — |
| CLI | validate (invalid) | medium/package-json | pass | — | FAIL: cannot load the fixture's resource documents | FAIL: cannot load the fixture's resource documents | — |
