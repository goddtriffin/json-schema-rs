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
| generated/deep-nesting | **374.128 ± 1.476 µs** | 9.651 ± 0.013 ms |
| generated/large-array | **44.948 ± 0.111 µs** | 1.033 ± 0.002 ms |
| generated/many-defs | **4.384 ± 0.010 ms** | 121.719 ± 0.558 ms |
| generated/wide-object | **5.606 ± 0.016 ms** | 147.803 ± 3.123 ms |
| large/api-catalog | **168.996 ± 3.703 µs** | 3.222 ± 0.193 ms |
| large/circleci | 1.698 ± 0.054 ms | — |
| massive/data-warehouse | **8.522 ± 0.148 ms** | 271.426 ± 7.677 ms |
| massive/k8s-deployment | 4.707 ± 0.047 ms | **1.820 ± 0.129 ms** |
| medium/package-json | 18.441 ± 1.327 ms | — |
| medium/user-profile | **62.485 ± 1.609 µs** | 1.471 ± 0.048 ms |
| small/config | **20.938 ± 0.383 µs** | 519.306 ± 10.547 µs |
| small/prettierrc | 328.923 ± 9.078 µs | — |

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
| generated/deep-nesting | 1.033 ± 0.002 ms | 205.312 ± 2.158 µs | **103.622 ± 2.302 µs** |
| generated/large-array | 44.138 ± 0.216 µs | 20.516 ± 0.056 µs | **10.691 ± 0.049 µs** |
| generated/many-defs | 13.717 ± 0.241 ms | 3.023 ± 0.069 ms | **1.627 ± 0.003 ms** |
| generated/wide-object | 23.088 ± 0.125 ms | 2.484 ± 0.009 ms | **1.314 ± 0.008 ms** |
| large/api-catalog | 177.853 ± 0.461 µs | 104.673 ± 1.810 µs | **43.764 ± 0.241 µs** |
| large/circleci | 2.855 ± 0.020 ms | 1.024 ± 0.021 ms | **508.604 ± 2.114 µs** |
| massive/data-warehouse | 44.500 ± 0.893 ms | 6.371 ± 0.234 ms | **2.636 ± 0.050 ms** |
| massive/k8s-deployment | 7.362 ± 0.152 ms | 1.584 ± 0.006 ms | **737.667 ± 7.687 µs** |
| medium/package-json | 68.335 ± 1.424 ms | 13.986 ± 0.103 ms | **5.876 ± 0.068 ms** |
| medium/user-profile | 66.302 ± 0.151 µs | 40.272 ± 0.764 µs | **16.324 ± 0.069 µs** |
| small/config | 27.018 ± 0.218 µs | 19.111 ± 0.317 µs | **7.529 ± 0.046 µs** |
| small/prettierrc | 205.299 ± 0.531 µs | 110.724 ± 1.354 µs | **56.317 ± 0.466 µs** |

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
| generated/deep-nesting | 6.811 ± 0.099 µs | **2.425 ± 0.037 µs** | 3.029 ± 0.022 µs |
| generated/large-array | 18.016 ± 0.276 ms | **7.246 ± 0.043 ms** | 8.860 ± 0.128 ms |
| generated/many-defs | 1.718 ± 0.059 ms | **624.764 ± 11.199 µs** | 759.803 ± 18.632 µs |
| generated/wide-object | 169.435 ± 0.615 µs | 110.707 ± 2.360 µs | **54.855 ± 0.992 µs** |
| large/api-catalog | 568.227 ± 4.958 ns | **241.737 ± 2.286 ns** | 418.248 ± 5.741 ns |
| large/circleci | 2.234 ± 0.054 µs | **473.067 ± 3.665 ns** | 1.017 ± 0.023 µs |
| massive/data-warehouse | 69.959 ± 0.857 µs | **24.040 ± 0.124 µs** | 46.202 ± 0.667 µs |
| massive/k8s-deployment | 518.120 ± 3.821 ns | **213.007 ± 3.072 ns** | 508.029 ± 11.899 ns |
| medium/package-json | 538.068 ± 1.531 ns | **229.905 ± 5.477 ns** | 486.335 ± 0.905 ns |
| medium/user-profile | 844.053 ± 5.809 ns | **376.841 ± 4.108 ns** | 502.731 ± 7.640 ns |
| small/config | 228.235 ± 1.834 ns | **113.190 ± 1.054 ns** | 140.242 ± 2.688 ns |
| small/prettierrc | 723.303 ± 5.340 ns | **176.622 ± 1.508 ns** | 281.324 ± 7.193 ns |

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
| generated/deep-nesting | 7.797 ± 0.021 µs | **2.804 ± 0.011 µs** | 5.172 ± 0.026 µs |
| generated/large-array | 17.885 ± 0.110 ms | **7.287 ± 0.057 ms** | 8.933 ± 0.232 ms |
| generated/many-defs | 1.715 ± 0.012 ms | **621.471 ± 2.356 µs** | 743.474 ± 10.109 µs |
| generated/wide-object | 171.325 ± 1.058 µs | 112.551 ± 1.634 µs | **55.298 ± 0.396 µs** |
| large/api-catalog | 668.450 ± 11.716 ns | **394.283 ± 3.550 ns** | 1.036 ± 0.006 µs |
| large/circleci | 130.268 ± 1.460 ns | 128.230 ± 0.728 ns | 269.418 ± 2.141 ns |
| massive/data-warehouse | 181.246 ± 2.043 µs | **139.122 ± 0.479 µs** | 278.511 ± 4.329 µs |
| massive/k8s-deployment | 177.649 ± 0.990 ns | **136.476 ± 2.496 ns** | 246.591 ± 7.778 ns |
| medium/package-json | 172.855 ± 0.852 ns | **149.985 ± 0.889 ns** | 242.795 ± 1.507 ns |
| medium/user-profile | 372.505 ± 5.605 ns | **325.889 ± 4.232 ns** | 554.506 ± 5.998 ns |
| small/config | 290.985 ± 2.165 ns | **248.084 ± 3.416 ns** | 552.692 ± 5.933 ns |
| small/prettierrc | **531.985 ± 1.221 ns** | 898.397 ± 6.893 ns | 1.522 ± 0.017 µs |

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
| generated/deep-nesting | **2.994 ± 0.270 ms** | 108.771 ± 1.595 ms |
| generated/large-array | **1.918 ± 0.049 ms** | 87.322 ± 1.007 ms |
| generated/many-defs | **7.821 ± 0.198 ms** | 442.489 ± 13.548 ms |
| generated/wide-object | **9.163 ± 0.213 ms** | 510.278 ± 3.126 ms |
| large/api-catalog | **2.255 ± 0.113 ms** | 95.147 ± 2.067 ms |
| large/circleci | 4.431 ± 0.112 ms | — |
| massive/data-warehouse | **12.501 ± 0.367 ms** | 794.411 ± 7.134 ms |
| massive/k8s-deployment | **8.293 ± 0.119 ms** | 87.267 ± 0.613 ms |
| medium/package-json | 34.662 ± 0.528 ms | — |
| medium/user-profile | **1.982 ± 0.059 ms** | 91.562 ± 2.387 ms |
| small/config | **1.816 ± 0.043 ms** | 86.689 ± 2.528 ms |
| small/prettierrc | 2.491 ± 0.052 ms | — |

### codegen — peak RSS

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | **4.91 MiB** | 38.45 ± 0.23 MiB |
| generated/large-array | **3.72 MiB** | 33.31 ± 0.01 MiB |
| generated/many-defs | **8.69 ± 0.02 MiB** | 98.16 ± 0.01 MiB |
| generated/wide-object | **11.50 ± 0.15 MiB** | 116.56 ± 0.67 MiB |
| large/api-catalog | **4.27 ± 0.01 MiB** | 34.64 MiB |
| large/circleci | 6.73 ± 0.01 MiB | — |
| massive/data-warehouse | **11.70 ± 0.07 MiB** | 176.03 ± 0.08 MiB |
| massive/k8s-deployment | **8.67 ± 0.01 MiB** | 31.78 MiB |
| medium/package-json | 19.81 ± 0.06 MiB | — |
| medium/user-profile | **3.84 ± 0.02 MiB** | 33.55 MiB |
| small/config | **3.56 ± 0.00 MiB** | 32.91 ± 0.02 MiB |
| small/prettierrc | 4.66 ± 0.01 MiB | — |

### validate (valid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.577 ± 0.488 ms † | 4.194 ± 0.091 ms | **1.869 ± 0.057 ms** |
| generated/large-array | 58.705 ± 0.768 ms | 44.910 ± 1.220 ms | 44.320 ± 0.481 ms |
| generated/many-defs | 23.056 ± 0.198 ms | 11.824 ± 0.856 ms | **7.505 ± 0.066 ms** |
| generated/wide-object | 27.814 ± 0.276 ms | 8.046 ± 0.081 ms | **3.898 ± 0.042 ms** |
| large/api-catalog | 3.179 ± 0.050 ms | 3.987 ± 0.133 ms | **1.813 ± 0.041 ms** |
| large/circleci | 6.997 ± 0.224 ms | 5.820 ± 0.129 ms | **2.608 ± 0.046 ms** |
| massive/data-warehouse | 50.112 ± 0.258 ms | 12.056 ± 0.251 ms | **5.517 ± 0.137 ms** |
| massive/k8s-deployment | 12.020 ± 0.132 ms | 284.595 ± 21.367 ms | **2.985 ± 0.050 ms** |
| medium/package-json | 75.559 ± 0.631 ms | — | **7.663 ± 0.105 ms** |
| medium/user-profile | 3.032 ± 0.075 ms | 3.790 ± 0.169 ms | **1.708 ± 0.055 ms** |
| small/config | 2.971 ± 0.050 ms | 3.838 ± 0.076 ms | **1.704 ± 0.058 ms** |
| small/prettierrc | 3.191 ± 0.068 ms | 3.848 ± 0.115 ms | **1.825 ± 0.044 ms** |

### validate (valid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.97 MiB | 9.89 ± 0.01 MiB | **3.97 MiB** |
| generated/large-array | **55.78 ± 0.00 MiB** | 64.69 MiB | 59.50 ± 0.01 MiB |
| generated/many-defs | 19.06 ± 0.01 MiB | 21.77 ± 0.15 MiB | **13.02 ± 0.01 MiB** |
| generated/wide-object | 15.11 MiB | 15.28 ± 0.01 MiB | **7.17 MiB** |
| large/api-catalog | 5.89 MiB | 10.36 ± 0.01 MiB | **3.73 ± 0.01 MiB** |
| large/circleci | 9.08 ± 0.01 MiB | 13.09 ± 0.02 MiB | **5.30 MiB** |
| massive/data-warehouse | 22.03 ± 0.03 MiB | 20.97 ± 0.02 MiB | **9.31 ± 0.01 MiB** |
| massive/k8s-deployment | 11.80 ± 0.01 MiB | 20.55 ± 0.02 MiB | **5.94 ± 0.01 MiB** |
| medium/package-json | 28.69 MiB | — | **11.41 ± 0.03 MiB** |
| medium/user-profile | 5.72 MiB | 10.08 MiB | **3.62 ± 0.01 MiB** |
| small/config | 5.72 ± 0.01 MiB | 9.86 ± 0.00 MiB | **3.53 MiB** |
| small/prettierrc | 5.84 ± 0.02 MiB | 8.97 MiB | **3.69 ± 0.01 MiB** |

### validate (invalid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.284 ± 0.134 ms | 4.181 ± 0.059 ms | **1.856 ± 0.035 ms** |
| generated/large-array | 58.588 ± 0.989 ms | 44.581 ± 6.771 ms † | 43.888 ± 0.376 ms |
| generated/many-defs | 23.438 ± 0.386 ms | 11.458 ± 0.103 ms | **7.456 ± 0.055 ms** |
| generated/wide-object | 27.777 ± 0.469 ms | 8.078 ± 0.057 ms | **3.892 ± 0.116 ms** |
| large/api-catalog | 3.208 ± 0.051 ms | 3.941 ± 0.034 ms | **1.795 ± 0.031 ms** |
| large/circleci | 6.943 ± 0.135 ms | 5.771 ± 0.077 ms | **2.603 ± 0.030 ms** |
| massive/data-warehouse | 51.279 ± 0.375 ms | 12.106 ± 0.206 ms | **5.633 ± 0.125 ms** |
| massive/k8s-deployment | 12.049 ± 0.135 ms | 266.989 ± 30.861 ms † | **2.960 ± 0.030 ms** |
| medium/package-json | 77.202 ± 1.433 ms | — | **8.180 ± 0.496 ms** |
| medium/user-profile | 3.022 ± 0.040 ms | 3.907 ± 0.061 ms | **1.741 ± 0.030 ms** |
| small/config | 2.950 ± 0.159 ms | 3.801 ± 0.101 ms | **1.669 ± 0.034 ms** |
| small/prettierrc | 3.229 ± 0.060 ms | 3.831 ± 0.065 ms | **1.766 ± 0.033 ms** |

### validate (invalid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.98 MiB | 10.02 ± 0.01 MiB | **4.05 ± 0.01 MiB** |
| generated/large-array | **55.81 ± 0.02 MiB** | 64.80 MiB | 59.58 MiB |
| generated/many-defs | 19.12 ± 0.01 MiB | 21.42 ± 0.01 MiB | **13.08 ± 0.01 MiB** |
| generated/wide-object | 15.11 ± 0.01 MiB | 15.42 MiB | **7.25 MiB** |
| large/api-catalog | 5.91 ± 0.00 MiB | 10.50 MiB | **3.80 ± 0.00 MiB** |
| large/circleci | 9.08 ± 0.01 MiB | 13.11 ± 0.04 MiB | **5.36 MiB** |
| massive/data-warehouse | 22.52 ± 0.01 MiB | 21.03 ± 0.02 MiB | **9.50 ± 0.01 MiB** |
| massive/k8s-deployment | 11.81 MiB | 20.48 ± 0.01 MiB | **5.98 ± 0.01 MiB** |
| medium/package-json | 28.83 MiB | — | **11.44 ± 0.01 MiB** |
| medium/user-profile | 5.72 ± 0.01 MiB | 10.20 ± 0.00 MiB | **3.67 MiB** |
| small/config | 5.75 MiB | 10.02 ± 0.01 MiB | **3.64 MiB** |
| small/prettierrc | 5.86 ± 0.02 MiB | 9.11 ± 0.02 MiB | **3.77 ± 0.01 MiB** |

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
