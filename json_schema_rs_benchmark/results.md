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
| generated/deep-nesting | **372.892 ± 1.565 µs** | 9.585 ± 0.049 ms |
| generated/large-array | **44.750 ± 0.410 µs** | 1.054 ± 0.021 ms |
| generated/many-defs | **4.352 ± 0.021 ms** | 121.357 ± 1.350 ms |
| generated/wide-object | **5.569 ± 0.024 ms** | 146.770 ± 0.523 ms |
| large/api-catalog | **163.361 ± 0.273 µs** | 3.099 ± 0.012 ms |
| large/circleci | 1.677 ± 0.006 ms | — |
| massive/data-warehouse | **8.411 ± 0.078 ms** | 269.317 ± 5.043 ms |
| massive/k8s-deployment | 4.781 ± 0.263 ms | **1.809 ± 0.015 ms** |
| medium/package-json | 16.208 ± 0.261 ms | — |
| medium/user-profile | **60.847 ± 0.256 µs** | 1.419 ± 0.023 ms |
| small/config | **20.552 ± 0.149 µs** | 509.375 ± 0.787 µs |
| small/prettierrc | 324.352 ± 1.910 µs | — |

### codegen — peak heap

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | **343.19 KiB** | 12.65 MiB |
| generated/large-array | **31.29 KiB** | 1.23 MiB |
| generated/many-defs | **3.28 MiB** | 133.38 MiB |
| generated/wide-object | **4.66 MiB** | 162.36 MiB |
| large/api-catalog | **96.40 KiB** | 3.64 MiB |
| large/circleci | 1.55 MiB | — |
| massive/data-warehouse | **5.65 MiB** | 288.90 MiB |
| massive/k8s-deployment | 3.42 MiB | **2.79 MiB** |
| medium/package-json | 15.65 MiB | — |
| medium/user-profile | **41.86 KiB** | 1.71 MiB |
| small/config | **16.29 KiB** | 639.58 KiB |
| small/prettierrc | 190.39 KiB | — |

### schema compile — time

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 1.014 ± 0.004 ms | 206.214 ± 1.297 µs | **99.792 ± 0.246 µs** |
| generated/large-array | 44.445 ± 0.051 µs | 20.573 ± 0.040 µs | **10.527 ± 0.026 µs** |
| generated/many-defs | 13.606 ± 0.116 ms | 2.971 ± 0.005 ms | **1.629 ± 0.005 ms** |
| generated/wide-object | 23.125 ± 0.076 ms | 2.498 ± 0.008 ms | **1.290 ± 0.002 ms** |
| large/api-catalog | 177.879 ± 0.908 µs | 103.009 ± 1.127 µs | **43.147 ± 0.092 µs** |
| large/circleci | 2.863 ± 0.018 ms | 1.018 ± 0.015 ms | **507.262 ± 2.920 µs** |
| massive/data-warehouse | 44.166 ± 0.209 ms | 6.221 ± 0.022 ms | **2.603 ± 0.007 ms** |
| massive/k8s-deployment | 7.334 ± 0.014 ms | 1.579 ± 0.004 ms | **745.130 ± 1.211 µs** |
| medium/package-json | 68.762 ± 0.201 ms | 14.119 ± 0.149 ms | **5.801 ± 0.018 ms** |
| medium/user-profile | 66.262 ± 0.033 µs | 39.524 ± 0.323 µs | **16.352 ± 0.019 µs** |
| small/config | 26.763 ± 0.291 µs | 18.745 ± 0.068 µs | **7.442 ± 0.042 µs** |
| small/prettierrc | 202.944 ± 1.456 µs | 109.738 ± 1.106 µs | **56.389 ± 0.430 µs** |

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
| generated/deep-nesting | 6.645 ± 0.006 µs | **2.433 ± 0.036 µs** | 3.040 ± 0.046 µs |
| generated/large-array | 18.018 ± 0.174 ms | **7.436 ± 0.101 ms** | 8.775 ± 0.314 ms |
| generated/many-defs | 1.720 ± 0.004 ms | **610.918 ± 1.219 µs** | 744.347 ± 3.177 µs |
| generated/wide-object | 170.724 ± 0.716 µs | 108.991 ± 1.836 µs | **54.503 ± 1.490 µs** |
| large/api-catalog | 566.306 ± 4.343 ns | **244.664 ± 3.921 ns** | 429.730 ± 4.803 ns |
| large/circleci | 2.262 ± 0.016 µs | **472.947 ± 5.807 ns** | 1.004 ± 0.016 µs |
| massive/data-warehouse | 68.762 ± 0.440 µs | **23.482 ± 0.059 µs** | 45.603 ± 0.309 µs |
| massive/k8s-deployment | 518.584 ± 1.843 ns | **213.633 ± 2.085 ns** | 502.814 ± 6.444 ns |
| medium/package-json | 537.603 ± 3.380 ns | **233.196 ± 2.041 ns** | 490.094 ± 3.638 ns |
| medium/user-profile | 849.575 ± 6.915 ns | **373.911 ± 4.636 ns** | 501.669 ± 6.894 ns |
| small/config | 225.128 ± 1.632 ns | **115.312 ± 1.100 ns** | 144.756 ± 3.124 ns |
| small/prettierrc | 735.892 ± 8.206 ns | **171.889 ± 2.219 ns** | 287.563 ± 3.212 ns |

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
| generated/deep-nesting | 7.625 ± 0.040 µs | **2.779 ± 0.010 µs** | 5.365 ± 0.114 µs |
| generated/large-array | 17.918 ± 0.147 ms | **7.388 ± 0.107 ms** | 8.657 ± 0.104 ms |
| generated/many-defs | 1.728 ± 0.010 ms | **615.773 ± 2.550 µs** | 746.124 ± 6.269 µs |
| generated/wide-object | 171.816 ± 1.759 µs | 108.303 ± 0.538 µs | **54.212 ± 0.417 µs** |
| large/api-catalog | 665.603 ± 4.026 ns | **394.801 ± 2.855 ns** | 1.047 ± 0.011 µs |
| large/circleci | **129.217 ± 1.633 ns** | 134.681 ± 2.161 ns | 295.518 ± 28.498 ns |
| massive/data-warehouse | 180.390 ± 1.180 µs | **140.539 ± 2.357 µs** | 278.921 ± 2.987 µs |
| massive/k8s-deployment | 175.529 ± 1.097 ns | **138.714 ± 2.857 ns** | 242.488 ± 2.100 ns |
| medium/package-json | 172.065 ± 2.331 ns | **155.358 ± 2.215 ns** | 244.692 ± 6.610 ns |
| medium/user-profile | 362.214 ± 0.379 ns | **318.172 ± 1.938 ns** | 534.680 ± 11.238 ns |
| small/config | 287.098 ± 0.986 ns | **246.097 ± 0.603 ns** | 529.669 ± 6.953 ns |
| small/prettierrc | **529.237 ± 1.311 ns** | 896.677 ± 3.232 ns | 1.500 ± 0.011 µs |

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
| generated/deep-nesting | **3.025 ± 0.287 ms** | 110.706 ± 0.674 ms |
| generated/large-array | **2.070 ± 0.057 ms** | 89.235 ± 0.801 ms |
| generated/many-defs | **8.069 ± 0.159 ms** | 439.808 ± 1.616 ms |
| generated/wide-object | **9.211 ± 0.205 ms** | 509.577 ± 3.167 ms |
| large/api-catalog | **2.281 ± 0.073 ms** | 96.825 ± 1.554 ms |
| large/circleci | 4.657 ± 0.067 ms | — |
| massive/data-warehouse | **12.857 ± 0.224 ms** | 789.947 ± 18.401 ms |
| massive/k8s-deployment | **8.367 ± 0.165 ms** | 88.457 ± 1.364 ms |
| medium/package-json | 35.521 ± 0.313 ms | — |
| medium/user-profile | **1.959 ± 0.061 ms** | 92.387 ± 1.113 ms |
| small/config | **1.917 ± 0.126 ms** | 89.554 ± 1.609 ms |
| small/prettierrc | 2.664 ± 0.170 ms | — |

### codegen — peak RSS

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | **5.11 MiB** | 38.00 MiB |
| generated/large-array | **3.77 ± 0.01 MiB** | 33.30 MiB |
| generated/many-defs | **8.77 MiB** | 98.16 MiB |
| generated/wide-object | **11.61 ± 0.14 MiB** | 115.20 ± 0.17 MiB |
| large/api-catalog | **4.28 ± 0.01 MiB** | 34.69 ± 0.02 MiB |
| large/circleci | 6.70 ± 0.02 MiB | — |
| massive/data-warehouse | **11.44 ± 0.02 MiB** | 175.94 ± 0.02 MiB |
| massive/k8s-deployment | **8.75 ± 0.01 MiB** | 31.80 MiB |
| medium/package-json | 19.72 ± 0.05 MiB | — |
| medium/user-profile | **3.92 MiB** | 33.55 ± 0.02 MiB |
| small/config | **3.62 MiB** | 32.94 ± 0.02 MiB |
| small/prettierrc | 4.69 ± 0.03 MiB | — |

### validate (valid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.503 ± 0.477 ms † | 4.395 ± 0.150 ms | **1.915 ± 0.048 ms** |
| generated/large-array | 58.851 ± 0.304 ms | 44.901 ± 0.276 ms | 43.999 ± 0.530 ms |
| generated/many-defs | 23.140 ± 0.746 ms | 11.660 ± 0.576 ms | **7.334 ± 0.348 ms** |
| generated/wide-object | 27.856 ± 0.406 ms | 8.074 ± 0.139 ms | **3.905 ± 0.082 ms** |
| large/api-catalog | 3.280 ± 0.207 ms | 4.224 ± 0.128 ms | **1.886 ± 0.159 ms** |
| large/circleci | 6.992 ± 0.129 ms | 5.818 ± 0.136 ms | **2.761 ± 0.118 ms** |
| massive/data-warehouse | 49.703 ± 0.297 ms | 11.980 ± 0.158 ms | **5.405 ± 0.104 ms** |
| massive/k8s-deployment | 11.894 ± 0.133 ms | 278.404 ± 29.138 ms † | **2.988 ± 0.103 ms** |
| medium/package-json | 75.072 ± 0.505 ms | — | — |
| medium/user-profile | 3.091 ± 0.102 ms | 3.969 ± 0.193 ms | **1.757 ± 0.065 ms** |
| small/config | 3.126 ± 0.097 ms | 3.814 ± 0.110 ms | **1.721 ± 0.084 ms** |
| small/prettierrc | 3.246 ± 0.104 ms | 3.964 ± 0.138 ms | **1.836 ± 0.098 ms** |

### validate (valid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.97 ± 0.01 MiB | 9.89 ± 0.02 MiB | **3.94 MiB** |
| generated/large-array | **55.80 MiB** | 64.70 MiB | 59.47 MiB |
| generated/many-defs | 19.08 ± 0.00 MiB | 21.34 ± 0.02 MiB | **12.91 MiB** |
| generated/wide-object | 15.11 ± 0.02 MiB | 15.27 ± 0.01 MiB | **7.19 MiB** |
| large/api-catalog | 5.89 ± 0.01 MiB | 10.36 MiB | **3.77 ± 0.02 MiB** |
| large/circleci | 9.19 MiB | 13.05 MiB | **5.38 ± 0.01 MiB** |
| massive/data-warehouse | 22.00 ± 0.02 MiB | 20.95 ± 0.01 MiB | **9.36 ± 0.01 MiB** |
| massive/k8s-deployment | 11.80 MiB | 20.67 MiB | **6.00 MiB** |
| medium/package-json | 27.67 ± 0.01 MiB | — | — |
| medium/user-profile | 5.72 ± 0.01 MiB | 10.09 MiB | **3.64 ± 0.00 MiB** |
| small/config | 5.72 MiB | 9.86 ± 0.01 MiB | **3.55 MiB** |
| small/prettierrc | 5.84 ± 0.02 MiB | 8.98 ± 0.02 MiB | **3.67 ± 0.01 MiB** |

### validate (invalid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.382 ± 0.128 ms | 4.381 ± 0.166 ms | **1.948 ± 0.071 ms** |
| generated/large-array | 58.951 ± 0.786 ms | 46.058 ± 0.898 ms | 44.093 ± 1.149 ms |
| generated/many-defs | 23.247 ± 0.205 ms | 11.436 ± 0.138 ms | **7.164 ± 0.057 ms** |
| generated/wide-object | 28.027 ± 0.295 ms | 8.097 ± 0.132 ms | **3.879 ± 0.120 ms** |
| large/api-catalog | 3.314 ± 0.154 ms | 4.083 ± 0.143 ms | **1.887 ± 0.065 ms** |
| large/circleci | 6.886 ± 0.118 ms | 5.712 ± 0.089 ms | **2.676 ± 0.062 ms** |
| massive/data-warehouse | 50.801 ± 0.405 ms | 12.012 ± 0.228 ms | **5.485 ± 0.108 ms** |
| massive/k8s-deployment | 11.948 ± 0.125 ms | 284.572 ± 46.207 ms † | **3.014 ± 0.148 ms** |
| medium/package-json | 74.736 ± 0.433 ms | — | — |
| medium/user-profile | 3.126 ± 0.367 ms † | 3.913 ± 0.231 ms | **1.830 ± 0.072 ms** |
| small/config | 3.022 ± 0.091 ms | 3.879 ± 0.555 ms † | **1.686 ± 0.045 ms** |
| small/prettierrc | 3.231 ± 0.133 ms | 3.871 ± 0.400 ms † | **1.823 ± 0.057 ms** |

### validate (invalid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.98 MiB | 10.03 ± 0.02 MiB | **4.02 MiB** |
| generated/large-array | **55.80 ± 0.02 MiB** | 64.81 MiB | 59.52 MiB |
| generated/many-defs | 19.09 MiB | 21.39 ± 0.03 MiB | **12.94 ± 0.01 MiB** |
| generated/wide-object | 15.09 MiB | 15.42 ± 0.01 MiB | **7.25 ± 0.01 MiB** |
| large/api-catalog | 5.91 MiB | 10.48 ± 0.01 MiB | **3.77 ± 0.01 MiB** |
| large/circleci | 9.05 ± 0.00 MiB | 13.03 ± 0.03 MiB | **5.38 ± 0.01 MiB** |
| massive/data-warehouse | 22.75 ± 0.12 MiB | 21.05 MiB | **9.58 ± 0.01 MiB** |
| massive/k8s-deployment | 11.81 MiB | 20.53 MiB | **5.97 MiB** |
| medium/package-json | 27.67 MiB | — | — |
| medium/user-profile | 5.73 MiB | 10.22 MiB | **3.64 ± 0.01 MiB** |
| small/config | 5.75 MiB | 10.02 ± 0.02 MiB | **3.59 ± 0.00 MiB** |
| small/prettierrc | 5.86 ± 0.00 MiB | 9.14 ± 0.01 MiB | **3.70 MiB** |

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
