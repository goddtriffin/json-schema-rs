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
| CLI | validate (valid) | time | jsonschemars | 11 / 12 |
| CLI | validate (valid) | peak RSS | jsonschemars | 11 / 12 |
| CLI | validate (invalid) | time | jsonschemars | 11 / 12 |
| CLI | validate (invalid) | peak RSS | jsonschemars | 11 / 12 |

## In-process (library calls)

### codegen — time

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | **3.081 ± 0.014 ms** | 9.542 ± 0.012 ms |
| generated/large-array | **139.002 ± 0.322 µs** | 1.031 ± 0.001 ms |
| generated/many-defs | **20.723 ± 0.036 ms** | 120.594 ± 2.606 ms |
| generated/wide-object | **18.286 ± 0.020 ms** | 145.808 ± 0.581 ms |
| large/api-catalog | **471.524 ± 0.436 µs** | 3.102 ± 0.028 ms |
| large/circleci | 6.851 ± 0.013 ms | — |
| massive/data-warehouse | **37.707 ± 0.111 ms** | 266.182 ± 1.374 ms |
| massive/k8s-deployment | 15.447 ± 0.071 ms | **1.795 ± 0.006 ms** |
| medium/package-json | 43.589 ± 0.045 ms | — |
| medium/user-profile | **191.929 ± 0.299 µs** | 1.423 ± 0.021 ms |
| small/config | **67.319 ± 0.845 µs** | 514.657 ± 2.254 µs |
| small/prettierrc | 854.438 ± 18.120 µs | — |

### codegen — peak heap

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | **1.52 MiB** | 12.65 MiB |
| generated/large-array | **102.38 KiB** | 1.23 MiB |
| generated/many-defs | **12.90 MiB** | 133.38 MiB |
| generated/wide-object | **13.11 MiB** | 162.36 MiB |
| large/api-catalog | **309.80 KiB** | 3.64 MiB |
| large/circleci | 3.90 MiB | — |
| massive/data-warehouse | **20.54 MiB** | 288.90 MiB |
| massive/k8s-deployment | 8.17 MiB | **2.79 MiB** |
| medium/package-json | 32.54 MiB | — |
| medium/user-profile | **138.08 KiB** | 1.71 MiB |
| small/config | **48.02 KiB** | 639.58 KiB |
| small/prettierrc | 528.44 KiB | — |

### schema compile — time

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 1.021 ± 0.003 ms | 205.548 ± 0.774 µs | **100.259 ± 0.459 µs** |
| generated/large-array | 44.542 ± 0.288 µs | 20.686 ± 0.027 µs | **10.551 ± 0.029 µs** |
| generated/many-defs | 13.731 ± 0.080 ms | 3.012 ± 0.037 ms | **1.627 ± 0.009 ms** |
| generated/wide-object | 23.206 ± 0.039 ms | 2.499 ± 0.026 ms | **1.297 ± 0.006 ms** |
| large/api-catalog | 177.389 ± 0.319 µs | 103.171 ± 0.218 µs | **43.430 ± 0.078 µs** |
| large/circleci | 2.855 ± 0.003 ms | 1.021 ± 0.003 ms | **513.702 ± 0.740 µs** |
| massive/data-warehouse | 44.694 ± 0.163 ms | 6.286 ± 0.020 ms | **2.625 ± 0.007 ms** |
| massive/k8s-deployment | 7.269 ± 0.069 ms | 1.589 ± 0.001 ms | **747.408 ± 2.617 µs** |
| medium/package-json | 68.927 ± 0.649 ms | 13.937 ± 0.030 ms | **5.837 ± 0.035 ms** |
| medium/user-profile | 66.907 ± 0.292 µs | 40.301 ± 0.342 µs | **16.422 ± 0.031 µs** |
| small/config | 27.014 ± 0.058 µs | 19.040 ± 0.083 µs | **7.433 ± 0.026 µs** |
| small/prettierrc | 203.146 ± 1.195 µs | 109.265 ± 0.104 µs | **56.690 ± 0.065 µs** |

### schema compile — peak heap

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 636.04 KiB | 534.41 KiB | **152.84 KiB** |
| generated/large-array | 66.67 KiB | 34.10 KiB | **20.43 KiB** |
| generated/many-defs | 8.10 MiB | 3.88 MiB | **2.64 MiB** |
| generated/wide-object | 13.73 MiB | 4.03 MiB | **2.69 MiB** |
| large/api-catalog | 266.71 KiB | 151.24 KiB | **68.23 KiB** |
| large/circleci | 3.85 MiB | 1.75 MiB | **869.05 KiB** |
| massive/data-warehouse | 16.73 MiB | 9.42 MiB | **4.04 MiB** |
| massive/k8s-deployment | 7.45 MiB | 3.28 MiB | **1.45 MiB** |
| medium/package-json | 29.52 MiB | 21.86 MiB | **11.36 MiB** |
| medium/user-profile | 121.82 KiB | 59.13 KiB | **28.67 KiB** |
| small/config | 38.35 KiB | 24.08 KiB | **10.86 KiB** |
| small/prettierrc | 476.85 KiB | 261.04 KiB | **115.58 KiB** |

### validate (valid) — time

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.535 ± 0.015 µs | **2.438 ± 0.024 µs** | 3.001 ± 0.024 µs |
| generated/large-array | 17.766 ± 0.035 ms | **7.555 ± 0.035 ms** | 8.774 ± 0.072 ms |
| generated/many-defs | 1.702 ± 0.003 ms | **624.651 ± 5.741 µs** | 747.550 ± 3.497 µs |
| generated/wide-object | 165.551 ± 0.255 µs | 114.204 ± 1.533 µs | **54.132 ± 0.619 µs** |
| large/api-catalog | 561.860 ± 3.662 ns | **240.703 ± 1.751 ns** | 418.631 ± 7.228 ns |
| large/circleci | 2.237 ± 0.025 µs | **475.133 ± 3.878 ns** | 998.016 ± 8.171 ns |
| massive/data-warehouse | 68.854 ± 0.286 µs | **23.954 ± 0.054 µs** | 45.668 ± 0.392 µs |
| massive/k8s-deployment | 513.638 ± 1.060 ns | **214.313 ± 0.826 ns** | 501.916 ± 6.256 ns |
| medium/package-json | 538.000 ± 1.070 ns | **231.402 ± 0.788 ns** | 484.179 ± 0.234 ns |
| medium/user-profile | 836.608 ± 3.625 ns | **382.643 ± 3.190 ns** | 488.536 ± 1.980 ns |
| small/config | 223.121 ± 1.067 ns | **116.462 ± 1.156 ns** | 139.756 ± 2.656 ns |
| small/prettierrc | 723.997 ± 4.880 ns | **174.882 ± 0.874 ns** | 276.743 ± 5.442 ns |

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
| generated/deep-nesting | 7.640 ± 0.010 µs | **2.879 ± 0.006 µs** | 5.110 ± 0.020 µs |
| generated/large-array | 17.862 ± 0.078 ms | **7.590 ± 0.082 ms** | 8.596 ± 0.059 ms |
| generated/many-defs | 1.716 ± 0.006 ms | **624.306 ± 1.485 µs** | 745.776 ± 3.714 µs |
| generated/wide-object | 166.945 ± 0.396 µs | 115.919 ± 1.083 µs | **54.375 ± 0.148 µs** |
| large/api-catalog | 662.239 ± 2.960 ns | **395.145 ± 1.063 ns** | 1.036 ± 0.005 µs |
| large/circleci | **129.266 ± 0.302 ns** | 134.094 ± 1.641 ns | 270.789 ± 1.020 ns |
| massive/data-warehouse | 179.628 ± 0.765 µs | **136.374 ± 0.761 µs** | 271.711 ± 1.295 µs |
| massive/k8s-deployment | 177.678 ± 0.403 ns | **134.667 ± 0.555 ns** | 242.215 ± 1.850 ns |
| medium/package-json | 175.936 ± 0.627 ns | **150.444 ± 1.223 ns** | 243.402 ± 0.865 ns |
| medium/user-profile | 368.910 ± 1.478 ns | **319.251 ± 2.408 ns** | 544.546 ± 2.919 ns |
| small/config | 291.196 ± 0.702 ns | **245.010 ± 0.810 ns** | 531.804 ± 5.153 ns |
| small/prettierrc | **540.281 ± 0.907 ns** | 898.329 ± 12.542 ns | 1.508 ± 0.002 µs |

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
| generated/deep-nesting | **6.057 ± 0.623 ms †** | 107.325 ± 0.606 ms |
| generated/large-array | **2.065 ± 0.048 ms** | 86.932 ± 0.618 ms |
| generated/many-defs | **28.486 ± 0.178 ms** | 434.491 ± 2.401 ms |
| generated/wide-object | **25.411 ± 0.194 ms** | 505.365 ± 1.517 ms |
| large/api-catalog | **2.584 ± 0.039 ms** | 93.306 ± 0.986 ms |
| large/circleci | 10.763 ± 0.053 ms | — |
| massive/data-warehouse | **44.448 ± 0.511 ms** | 783.988 ± 13.562 ms |
| massive/k8s-deployment | **20.996 ± 0.170 ms** | 85.342 ± 1.137 ms |
| medium/package-json | 96.019 ± 1.093 ms | — |
| medium/user-profile | **2.202 ± 0.037 ms** | 87.274 ± 0.805 ms |
| small/config | **1.975 ± 0.098 ms** | 84.172 ± 0.807 ms |
| small/prettierrc | 3.063 ± 0.044 ms | — |

### codegen — peak RSS

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | **7.88 ± 0.00 MiB** | 38.02 MiB |
| generated/large-array | **4.70 MiB** | 33.30 ± 0.02 MiB |
| generated/many-defs | **26.05 MiB** | 98.05 ± 0.04 MiB |
| generated/wide-object | **26.66 ± 0.01 MiB** | 114.62 ± 0.01 MiB |
| large/api-catalog | **5.36 MiB** | 34.69 MiB |
| large/circleci | 11.22 ± 0.01 MiB | — |
| massive/data-warehouse | **35.23 ± 0.02 MiB** | 177.75 ± 0.76 MiB |
| massive/k8s-deployment | **17.50 ± 0.01 MiB** | 31.77 ± 0.01 MiB |
| medium/package-json | 39.17 MiB | — |
| medium/user-profile | **4.86 MiB** | 33.62 MiB |
| small/config | **4.41 ± 0.04 MiB** | 32.91 MiB |
| small/prettierrc | 5.58 MiB | — |

### validate (valid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.436 ± 0.451 ms † | 3.937 ± 0.045 ms | **1.767 ± 0.025 ms** |
| generated/large-array | 58.416 ± 1.061 ms | 44.383 ± 0.311 ms | 43.798 ± 0.614 ms |
| generated/many-defs | 22.719 ± 0.280 ms | 11.203 ± 0.065 ms | **7.178 ± 0.087 ms** |
| generated/wide-object | 27.709 ± 0.197 ms | 7.960 ± 0.088 ms | **3.714 ± 0.036 ms** |
| large/api-catalog | 2.999 ± 0.029 ms | 3.781 ± 0.036 ms | **1.700 ± 0.026 ms** |
| large/circleci | 6.724 ± 0.051 ms | 5.500 ± 0.064 ms | **2.485 ± 0.038 ms** |
| massive/data-warehouse | 49.546 ± 0.363 ms | 11.839 ± 0.098 ms | **5.185 ± 0.056 ms** |
| massive/k8s-deployment | 11.598 ± 0.080 ms | 291.019 ± 42.639 ms † | **2.775 ± 0.050 ms** |
| medium/package-json | 73.912 ± 0.246 ms | — | **7.271 ± 0.054 ms** |
| medium/user-profile | 2.847 ± 0.034 ms | 3.605 ± 0.047 ms | **1.645 ± 0.025 ms** |
| small/config | 2.764 ± 0.032 ms | 3.541 ± 0.060 ms | **1.608 ± 0.024 ms** |
| small/prettierrc | 3.020 ± 0.039 ms | 3.576 ± 0.046 ms | **1.693 ± 0.039 ms** |

### validate (valid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.97 MiB | 9.89 ± 0.02 MiB | **4.16 ± 0.01 MiB** |
| generated/large-array | **55.77 ± 0.01 MiB** | 64.69 ± 0.01 MiB | 59.69 ± 0.01 MiB |
| generated/many-defs | 19.09 ± 0.01 MiB | 21.33 ± 0.02 MiB | **13.17 MiB** |
| generated/wide-object | 15.08 ± 0.00 MiB | 15.28 MiB | **7.25 MiB** |
| large/api-catalog | 5.88 ± 0.01 MiB | 10.34 MiB | **3.92 MiB** |
| large/circleci | 9.06 MiB | 13.12 MiB | **5.42 ± 0.01 MiB** |
| massive/data-warehouse | 22.03 ± 0.01 MiB | 20.97 ± 0.03 MiB | **9.42 ± 0.01 MiB** |
| massive/k8s-deployment | 11.80 ± 0.01 MiB | 20.48 ± 0.01 MiB | **6.00 MiB** |
| medium/package-json | 27.64 ± 0.01 MiB | — | **11.56 ± 0.02 MiB** |
| medium/user-profile | 5.72 MiB | 10.06 MiB | **3.80 MiB** |
| small/config | 5.72 MiB | 9.86 ± 0.00 MiB | **3.69 MiB** |
| small/prettierrc | 5.84 ± 0.01 MiB | 8.95 ± 0.01 MiB | **3.92 MiB** |

### validate (invalid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 3.978 ± 0.048 ms | 4.055 ± 0.106 ms | **1.746 ± 0.024 ms** |
| generated/large-array | 58.179 ± 0.500 ms | 44.503 ± 0.282 ms | 43.425 ± 0.290 ms |
| generated/many-defs | 23.008 ± 0.485 ms | 11.162 ± 0.104 ms | **7.164 ± 0.061 ms** |
| generated/wide-object | 27.586 ± 0.167 ms | 7.868 ± 0.072 ms | **3.723 ± 0.032 ms** |
| large/api-catalog | 3.051 ± 0.028 ms | 3.807 ± 0.057 ms | **1.714 ± 0.023 ms** |
| large/circleci | 6.692 ± 0.056 ms | 5.550 ± 0.663 ms † | **2.476 ± 0.022 ms** |
| massive/data-warehouse | 50.528 ± 0.382 ms | 11.899 ± 0.109 ms | **5.363 ± 0.085 ms** |
| massive/k8s-deployment | 11.577 ± 0.068 ms | 277.839 ± 113.802 ms † | **2.770 ± 0.041 ms** |
| medium/package-json | 73.990 ± 0.291 ms | — | **7.308 ± 0.058 ms** |
| medium/user-profile | 2.828 ± 0.034 ms | 3.652 ± 0.054 ms | **1.625 ± 0.022 ms** |
| small/config | 2.815 ± 0.025 ms | 3.517 ± 0.052 ms | **1.617 ± 0.024 ms** |
| small/prettierrc | 3.042 ± 0.042 ms | 3.606 ± 0.060 ms | **1.707 ± 0.020 ms** |

### validate (invalid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.98 MiB | 9.97 ± 0.03 MiB | **4.20 ± 0.01 MiB** |
| generated/large-array | **55.80 ± 0.01 MiB** | 64.81 ± 0.01 MiB | 59.72 ± 0.01 MiB |
| generated/many-defs | 19.06 MiB | 21.45 ± 0.01 MiB | **13.20 ± 0.01 MiB** |
| generated/wide-object | 15.09 ± 0.01 MiB | 15.42 MiB | **7.33 MiB** |
| large/api-catalog | 5.89 MiB | 10.50 ± 0.02 MiB | **3.97 ± 0.02 MiB** |
| large/circleci | 9.06 MiB | 13.11 ± 0.03 MiB | **5.50 ± 0.01 MiB** |
| massive/data-warehouse | 22.75 MiB | 21.06 ± 0.01 MiB | **9.61 ± 0.02 MiB** |
| massive/k8s-deployment | 11.81 MiB | 20.45 ± 0.04 MiB | **6.02 ± 0.02 MiB** |
| medium/package-json | 27.66 ± 0.02 MiB | — | **11.53 ± 0.02 MiB** |
| medium/user-profile | 5.77 ± 0.03 MiB | 10.19 ± 0.02 MiB | **3.81 ± 0.01 MiB** |
| small/config | 5.75 MiB | 10.03 ± 0.02 MiB | **3.77 MiB** |
| small/prettierrc | 5.86 ± 0.02 MiB | 9.14 ± 0.01 MiB | **3.95 MiB** |

## Compatibility

220 of 228 attempted cells pass. Rows with at least one failure:

| Tier | Operation | Fixture | boon | jsonschema | jsonschema-cli | jsonschemars | typify |
|---|---|---|---|---|---|---|---|
| in-process | codegen | large/circleci | — | — | — | pass | FAIL: panic: not implemented: if/then/else schemas are not supported: { "if": { "required": [ "type" ], "properties": { "type": { "const": "release" } } }, "then": { "required": [ "plan_name" ], "properties": { "plan_name": { "type": "string", "markdownDescription": "Required plan name for release jobs." } }, "additionalProperties": true, "markdownDescription": "https://circleci.com/docs/reference/configuration-reference/#job-type\n\nRelease job." }, "else": { "if": { "required": [ "type" ], "properties": { "type": { "enum": [ "lock", "unlock" ] } } }, "then": { "required": [ "key" ], "properties": { "key": { "type": "string", "markdownDescription": "Required key for lock/unlock jobs." } }, "additionalProperties": true, "markdownDescription": "https://circleci.com/docs/reference/configuration-reference/#job-type\n\nA lock/unlock job" }, "else": { "if": { "required": [ "type" ], "properties": { "type": { "enum": [ "approval", "no-op" ] } } }, "then": { "additionalProperties": true, "markdownDescription": "https://circleci.com/docs/reference/configuration-reference/#job-type\n\nApproval/no-op jobs primarily act as workflow markers. Other keys like `steps:` are permitted but will be ignored in the final pipeline." }, "else": { "if": { "anyOf": [ { "not": { "required": [ "type" ] } }, { "required": [ "type" ], "properties": { "type": { "const": "build" } } } ], "markdownDescription": "https://circleci.com/docs/reference/configuration-reference/#job-type\n\nA build job. The job explicitly has type 'build' or defaults to build when no type specified." }, "then": { "anyOf": [ { "type": "object", "required": [ "executor" ], "markdownDescription": "A job that uses an executor defined elsewhere in the configuration." }, { "oneOf": [ { "type": "object", "required": [ "machine" ] }, { "type": "object", "required": [ "docker" ] }, { "type": "object", "required": [ "macos" ] } ] } ], "required": [ "steps" ], "properties": { "circleci_ip_ranges": { "type": "boolean", "markdownDescription": "Enable running jobs with a set of well-defined IP address ranges." }, "description": { "type": "string" }, "docker": { "type": "array", "items": { "type": "object", "required": [ "image" ], "properties": { "auth": { "type": "object", "required": [ "password", "username" ], "properties": { "password": { "type": "string" }, "username": { "type": "string" } }, "additionalProperties": false }, "aws_auth": { "oneOf": [ { "type": "object", "required": [ "aws_access_key_id", "aws_secret_access_key" ], "properties": { "aws_access_key_id": { "type": "string" }, "aws_secret_access_key": { "type": "string" } }, "additionalProperties": false }, { "type": "object", "required": [ "oidc_role_arn" ], "properties": { "oidc_role_arn": { "type": "string" } }, "additionalProperties": false } ] }, "command": { "oneOf": [ { "type": "string" }, { "type": "array", "items": { "type": "string" } } ] }, "entrypoint": { "oneOf": [ { "type": "string" }, { "type": "array", "items": { "type": "string" } } ] }, "environment": { "oneOf": [ { "type": [ "string", "null" ], "markdownDescription": "A map of environment variable names and values." }, { "type": "array", "items": { "$ref": "#/definitions/environment" }, "markdownDescription": "An array of strings in the form KEY=VALUE" }, { "type": "object", "additionalProperties": { "type": [ "string", "boolean", "number", "null" ] }, "markdownDescription": "The value should be a string, but we allow null, boolean and numbers too. Examples: `FOO: true` and `BAR: 2` `BAZ:`" } ] }, "image": { "type": "string" }, "name": { "type": "string" }, "user": { "type": "string" } }, "additionalProperties": false }, "minItems": 1 }, "environment": { "oneOf": [ { "type": [ "string", "null" ], "markdownDescription": "A map of environment variable names and values." }, { "type": "array", "items": { "$ref": "#/definitions/environment" }, "markdownDescription": "An array of strings in the form KEY=VALUE" }, { "type": "object", "additionalProperties": { "type": [ "string", "boolean", "number", "null" ] }, "markdownDescription": "The value should be a string, but we allow null, boolean and numbers too. Examples: `FOO: true` and `BAR: 2` `BAZ:`" } ] }, "executor": { "oneOf": [ { "type": "string", "markdownDescription": "https://circleci.com/docs/reference/reusing-config/#executor\n\nThe name of the executor to use (defined via the top level executors map or from an orb)." }, { "type": "object", "minProperties": 1, "required": [ "name" ], "properties": { "name": { "type": "string" } }, "additionalProperties": true, "markdownDescription": "Executor stanza to use for the job, with optional parameter overrides." } ] }, "machine": { "oneOf": [ { "anyOf": [ { "type": "string", "pattern": " *<< *parameters.([^ ]+) *>> *" }, { "type": "boolean" }, { "type": "number" }, { "type": "string" } ] }, { "type": "object", "properties": { "docker_layer_caching": { "anyOf": [ { "type": "string", "pattern": " *<< *parameters.([^ ]+) *>> *" }, { "type": "boolean" }, { "type": "number" }, { "type": "string" } ], "markdownDescription": "Set to `true` to enable [Docker Layer Caching](https://circleci.com/docs/docker-layer-caching). Note: If you haven't already, you must open a support ticket to have a CircleCI Sales representative contact you about enabling this feature on your account for an additional fee." }, "enabled": { "anyOf": [ { "type": "string", "pattern": " *<< *parameters.([^ ]+) *>> *" }, { "type": "boolean" }, { "type": "number" }, { "type": "string" } ], "markdownDescription": "A boolean or a template parameter evaluating to a boolean" }, "image": { "type": "string" }, "resource_class": { "type": "string" }, "shell": { "type": "string" } }, "additionalProperties": false } ], "markdownDescription": "Use the default machine executor image" }, "macos": { "type": "object", "required": [ "xcode" ], "properties": { "resource_class": { "type": "string" }, "shell": { "type": "string" }, "xcode": { "type": [ "string", "number" ] } }, "additionalProperties": false }, "parallelism": { "anyOf": [ { "oneOf": [ { "type": "string", "pattern": " *<< *parameters.([^ ]+) *>> *" }, { "type": "string", "pattern": " *<< *pipeline.parameters.([^ ]+) *>> *" } ] }, { "type": "integer" } ], "markdownDescription": "A integer or a parameter evaluating to a integer" }, "parameters": { "type": "object", "minProperties": 1, "additionalProperties": { "type": "object", "required": [ "type" ], "properties": { "default": { "oneOf": [ { "type": "string" }, { "type": "boolean" }, { "type": "integer" }, { "type": "array", "items": { "$ref": "#/definitions/step" } } ] }, "description": { "type": "string", "markdownDescription": "A description of the parameter" }, "enum": { "type": "array", "items": { "type": "string" }, "minItems": 1, "markdownDescription": "List of allowed values for an `enum` type parameter" }, "type": { "type": "string", "enum": [ "boolean", "string", "steps", "enum", "executor", "integer", "env_var_name" ], "markdownDescription": "The parameter type. See [Parameter Types](https://circleci.com/docs/reusing-config#parameter-syntax) for details:\n- `boolean`: A boolean parameter\n- `string`: A string parameter \n- `integer`: An integer parameter\n- `enum`: An enum parameter (list of allowed string values)\n- `executor`: An executor parameter (allows job to choose executor)\n- `steps`: A steps parameter (allows mixing predefined and user-defined steps)\n- `env_var_name`: An environment variable name parameter (must match POSIX_NAME regexp)" } }, "additionalProperties": false }, "markdownDescription": "https://circleci.com/docs/reusing-config#using-the-parameters-declaration\n\nA map of parameter keys." }, "resource_class": { "type": "string" }, "retention": { "type": "object", "properties": { "caches": { "type": "string", "pattern": "^([1-9]\|1[0-5])d$" } }, "additionalProperties": false }, "shell": { "oneOf": [ { "type": "string" }, { "type": "array", "items": { "type": "string" } } ] }, "steps": { "type": "array", "items": { "$ref": "#/definitions/step" }, "minItems": 1 }, "type": { "const": "build", "markdownDescription": "The job type. If not specified, defaults to build." }, "working_directory": { "type": "string", "markdownDescription": "In which directory to run the steps. (default: `~/project`. `project` is a literal string, not the name of the project.) You can also refer the directory with `$CIRCLE_WORKING_DIRECTORY` environment variable." } }, "additionalProperties": false, "markdownDescription": "Validation: The job must adhere to the structure of a build job." }, "else": false } } } } |
| in-process | codegen | medium/package-json | — | — | — | pass | FAIL: cannot load the fixture's resource documents |
| in-process | codegen | small/prettierrc | — | — | — | pass | FAIL: codegen error: value does not conform to the given schema |
| CLI | codegen | large/circleci | — | — | — | pass | FAIL: exit 101: not implemented: if/then/else schemas are not supported: { |
| CLI | codegen | medium/package-json | — | — | — | pass | FAIL: cannot load the fixture's resource documents |
| CLI | codegen | small/prettierrc | — | — | — | pass | FAIL: exit 1: value does not conform to the given schema |
| CLI | validate (valid) | medium/package-json | pass | — | FAIL: cannot load the fixture's resource documents | pass | — |
| CLI | validate (invalid) | medium/package-json | pass | — | FAIL: cannot load the fixture's resource documents | pass | — |
