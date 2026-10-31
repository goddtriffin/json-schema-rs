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
| generated/deep-nesting | **374.010 ± 4.469 µs** | 9.749 ± 0.130 ms |
| generated/large-array | **44.946 ± 0.356 µs** | 1.030 ± 0.010 ms |
| generated/many-defs | **4.369 ± 0.031 ms** | 122.111 ± 2.048 ms |
| generated/wide-object | **5.622 ± 0.033 ms** | 148.808 ± 2.224 ms |
| large/api-catalog | **164.186 ± 5.002 µs** | 3.084 ± 0.020 ms |
| large/circleci | 1.679 ± 0.018 ms | — |
| massive/data-warehouse | **8.495 ± 0.068 ms** | 271.539 ± 4.000 ms |
| massive/k8s-deployment | 4.696 ± 0.026 ms | **1.810 ± 0.024 ms** |
| medium/package-json | 16.278 ± 0.154 ms | — |
| medium/user-profile | **60.462 ± 0.564 µs** | 1.424 ± 0.029 ms |
| small/config | **20.495 ± 0.350 µs** | 508.946 ± 2.132 µs |
| small/prettierrc | 323.966 ± 3.438 µs | — |

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
| generated/deep-nesting | 1.028 ± 0.015 ms | 206.718 ± 2.479 µs | **101.065 ± 1.332 µs** |
| generated/large-array | 44.655 ± 0.300 µs | 20.464 ± 0.207 µs | **10.561 ± 0.061 µs** |
| generated/many-defs | 13.699 ± 0.181 ms | 3.007 ± 0.137 ms | **1.606 ± 0.018 ms** |
| generated/wide-object | 23.383 ± 0.224 ms | 2.511 ± 0.008 ms | **1.297 ± 0.005 ms** |
| large/api-catalog | 179.714 ± 3.113 µs | 103.491 ± 0.871 µs | **43.815 ± 0.109 µs** |
| large/circleci | 2.883 ± 0.038 ms | 1.032 ± 0.010 ms | **510.749 ± 5.664 µs** |
| massive/data-warehouse | 43.969 ± 0.438 ms | 6.313 ± 0.082 ms | **2.644 ± 0.055 ms** |
| massive/k8s-deployment | 7.348 ± 0.054 ms | 1.588 ± 0.022 ms | **744.587 ± 8.228 µs** |
| medium/package-json | 68.083 ± 1.020 ms | 14.073 ± 0.076 ms | **5.875 ± 0.152 ms** |
| medium/user-profile | 66.729 ± 0.654 µs | 39.699 ± 0.463 µs | **16.508 ± 0.509 µs** |
| small/config | 27.348 ± 0.314 µs | 18.954 ± 0.202 µs | **7.393 ± 0.044 µs** |
| small/prettierrc | 203.274 ± 0.664 µs | 110.908 ± 1.091 µs | **56.706 ± 0.323 µs** |

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
| generated/deep-nesting | 6.744 ± 0.066 µs | **2.382 ± 0.035 µs** | 2.998 ± 0.036 µs |
| generated/large-array | 18.564 ± 0.170 ms | **7.374 ± 0.098 ms** | 8.735 ± 0.962 ms † |
| generated/many-defs | 1.735 ± 0.009 ms | **623.384 ± 2.666 µs** | 750.019 ± 15.343 µs |
| generated/wide-object | 170.967 ± 2.133 µs | 107.973 ± 3.483 µs | **54.835 ± 0.584 µs** |
| large/api-catalog | 572.507 ± 2.265 ns | **243.995 ± 2.966 ns** | 421.717 ± 4.032 ns |
| large/circleci | 2.191 ± 0.041 µs | **472.331 ± 4.434 ns** | 1.009 ± 0.019 µs |
| massive/data-warehouse | 69.783 ± 0.290 µs | **23.728 ± 0.115 µs** | 44.756 ± 0.219 µs |
| massive/k8s-deployment | 527.872 ± 2.807 ns | **208.435 ± 1.602 ns** | 497.202 ± 1.422 ns |
| medium/package-json | 549.529 ± 4.280 ns | **233.666 ± 1.482 ns** | 484.748 ± 3.648 ns |
| medium/user-profile | 854.937 ± 13.305 ns | **375.841 ± 6.351 ns** | 492.511 ± 4.587 ns |
| small/config | 226.207 ± 4.834 ns | **112.131 ± 0.166 ns** | 150.586 ± 4.129 ns |
| small/prettierrc | 741.451 ± 8.874 ns | **174.024 ± 1.026 ns** | 281.563 ± 5.034 ns |

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
| generated/deep-nesting | 7.677 ± 0.025 µs | **2.814 ± 0.023 µs** | 6.314 ± 0.035 µs |
| generated/large-array | 18.410 ± 0.123 ms | **7.417 ± 0.066 ms** | 8.610 ± 0.108 ms |
| generated/many-defs | 1.728 ± 0.021 ms | **620.857 ± 8.363 µs** | 743.072 ± 4.124 µs |
| generated/wide-object | 169.986 ± 1.298 µs | 110.490 ± 1.508 µs | **55.550 ± 0.241 µs** |
| large/api-catalog | 659.486 ± 2.347 ns | **397.692 ± 2.611 ns** | 1.369 ± 0.014 µs |
| large/circleci | 127.394 ± 0.489 ns | 127.226 ± 1.593 ns | 301.862 ± 10.917 ns |
| massive/data-warehouse | 182.550 ± 1.416 µs | **138.576 ± 1.315 µs** | 353.023 ± 2.282 µs |
| massive/k8s-deployment | 180.683 ± 1.161 ns | **134.389 ± 1.376 ns** | 246.777 ± 2.825 ns |
| medium/package-json | 173.894 ± 0.451 ns | **149.864 ± 1.481 ns** | 252.648 ± 3.328 ns |
| medium/user-profile | 373.241 ± 4.374 ns | **311.597 ± 2.013 ns** | 568.931 ± 5.036 ns |
| small/config | 288.894 ± 1.442 ns | **248.182 ± 2.802 ns** | 599.527 ± 5.144 ns |
| small/prettierrc | **529.087 ± 3.001 ns** | 937.988 ± 7.783 ns | 1.995 ± 0.033 µs |

### validate (invalid) — peak heap

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 2.97 KiB | **0.75 KiB** | 6.81 KiB |
| generated/large-array | 1.37 KiB | **0.46 KiB** | 1.09 KiB |
| generated/many-defs | 7.77 KiB | **0.77 KiB** | 3.09 KiB |
| generated/wide-object | 0.74 KiB | **0.41 KiB** | 0.88 KiB |
| large/api-catalog | 1.44 KiB | **0.83 KiB** | 2.76 KiB |
| large/circleci | 0.74 KiB | **0.40 KiB** | 1.29 KiB |
| massive/data-warehouse | 467.89 KiB | **111.78 KiB** | 682.55 KiB |
| massive/k8s-deployment | 0.81 KiB | **0.48 KiB** | 0.88 KiB |
| medium/package-json | 0.74 KiB | **0.40 KiB** | 0.86 KiB |
| medium/user-profile | 1.30 KiB | **0.73 KiB** | 1.20 KiB |
| small/config | 1.30 KiB | **0.71 KiB** | 1.90 KiB |
| small/prettierrc | **2.38 KiB** | 2.69 KiB | 4.64 KiB |

## CLI (end to end)

### codegen — time

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | **2.887 ± 0.352 ms †** | 112.487 ± 2.067 ms |
| generated/large-array | **2.456 ± 0.232 ms** | 88.763 ± 0.781 ms |
| generated/many-defs | **7.895 ± 0.167 ms** | 441.725 ± 2.508 ms |
| generated/wide-object | **9.279 ± 0.307 ms** | 517.024 ± 6.293 ms |
| large/api-catalog | **2.283 ± 0.060 ms** | 97.763 ± 1.832 ms |
| large/circleci | 4.589 ± 0.056 ms | — |
| massive/data-warehouse | **12.684 ± 0.225 ms** | 798.643 ± 6.032 ms |
| massive/k8s-deployment | **8.657 ± 0.740 ms** | 87.922 ± 1.508 ms |
| medium/package-json | 35.306 ± 0.230 ms | — |
| medium/user-profile | **2.035 ± 0.069 ms** | 89.736 ± 1.400 ms |
| small/config | **1.896 ± 0.095 ms** | 87.941 ± 2.080 ms |
| small/prettierrc | 2.524 ± 0.086 ms | — |

### codegen — peak RSS

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | **4.92 ± 0.02 MiB** | 38.03 ± 0.01 MiB |
| generated/large-array | **3.75 ± 0.01 MiB** | 33.31 ± 0.00 MiB |
| generated/many-defs | **8.72 ± 0.01 MiB** | 98.11 MiB |
| generated/wide-object | **11.19 ± 0.01 MiB** | 115.59 MiB |
| large/api-catalog | **4.23 MiB** | 34.67 MiB |
| large/circleci | 6.66 ± 0.02 MiB | — |
| massive/data-warehouse | **11.58 MiB** | 176.02 ± 0.20 MiB |
| massive/k8s-deployment | **9.09 ± 0.13 MiB** | 31.80 ± 0.00 MiB |
| medium/package-json | 19.58 ± 0.04 MiB | — |
| medium/user-profile | **3.81 MiB** | 33.58 ± 0.01 MiB |
| small/config | **3.62 ± 0.00 MiB** | 32.92 ± 0.01 MiB |
| small/prettierrc | 4.61 ± 0.01 MiB | — |

### validate (valid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.619 ± 0.144 ms | 4.413 ± 0.112 ms | **1.920 ± 0.051 ms** |
| generated/large-array | 60.147 ± 1.086 ms | 45.734 ± 1.085 ms | 45.012 ± 0.537 ms |
| generated/many-defs | 22.979 ± 0.222 ms | 11.680 ± 0.455 ms | **7.255 ± 0.149 ms** |
| generated/wide-object | 28.036 ± 0.891 ms | 8.297 ± 0.233 ms | **4.106 ± 0.125 ms** |
| large/api-catalog | 3.342 ± 0.123 ms | 4.340 ± 0.417 ms | **1.948 ± 0.045 ms** |
| large/circleci | 6.960 ± 0.096 ms | 5.879 ± 0.129 ms | **2.725 ± 0.062 ms** |
| massive/data-warehouse | 50.056 ± 0.375 ms | 11.940 ± 0.094 ms | **5.470 ± 0.068 ms** |
| massive/k8s-deployment | 11.887 ± 0.128 ms | 288.643 ± 44.027 ms † | **3.102 ± 0.092 ms** |
| medium/package-json | 74.297 ± 1.012 ms | — | — |
| medium/user-profile | 3.132 ± 0.151 ms | 3.991 ± 0.250 ms | **1.788 ± 0.055 ms** |
| small/config | 2.992 ± 0.056 ms | 3.868 ± 0.069 ms | **1.779 ± 0.106 ms** |
| small/prettierrc | 3.236 ± 0.063 ms | 3.969 ± 0.419 ms † | **1.823 ± 0.037 ms** |

### validate (valid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 7.06 MiB | 9.86 ± 0.02 MiB | **3.98 MiB** |
| generated/large-array | **55.77 ± 0.02 MiB** | 64.67 ± 0.01 MiB | 59.50 MiB |
| generated/many-defs | 20.73 MiB | 21.36 ± 0.01 MiB | **12.94 ± 0.01 MiB** |
| generated/wide-object | 15.11 MiB | 15.28 ± 0.01 MiB | **7.19 ± 0.02 MiB** |
| large/api-catalog | 5.92 ± 0.01 MiB | 10.34 ± 0.01 MiB | **3.73 ± 0.01 MiB** |
| large/circleci | 9.08 ± 0.01 MiB | 13.05 ± 0.04 MiB | **5.28 ± 0.01 MiB** |
| massive/data-warehouse | 22.05 MiB | 21.00 ± 0.03 MiB | **9.38 ± 0.02 MiB** |
| massive/k8s-deployment | 11.78 ± 0.01 MiB | 20.67 MiB | **5.92 MiB** |
| medium/package-json | 27.67 MiB | — | — |
| medium/user-profile | 5.73 ± 0.01 MiB | 10.08 ± 0.01 MiB | **3.62 MiB** |
| small/config | 5.72 ± 0.01 MiB | 9.86 MiB | **3.56 MiB** |
| small/prettierrc | 5.83 MiB | 8.95 ± 0.01 MiB | **3.70 ± 0.01 MiB** |

### validate (invalid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.946 ± 1.887 ms † | 4.256 ± 0.083 ms | **2.037 ± 0.102 ms** |
| generated/large-array | 58.821 ± 0.610 ms | 45.514 ± 0.486 ms | 46.349 ± 1.037 ms |
| generated/many-defs | 22.900 ± 0.192 ms | 11.478 ± 0.114 ms | **7.259 ± 0.134 ms** |
| generated/wide-object | 28.204 ± 0.270 ms | 8.113 ± 0.099 ms | **3.988 ± 0.112 ms** |
| large/api-catalog | 3.364 ± 0.171 ms | 4.116 ± 0.156 ms | **1.985 ± 0.070 ms** |
| large/circleci | 7.001 ± 0.193 ms | 5.979 ± 0.475 ms | **2.718 ± 0.068 ms** |
| massive/data-warehouse | 51.876 ± 1.462 ms | 12.478 ± 0.656 ms | **5.713 ± 0.128 ms** |
| massive/k8s-deployment | 11.879 ± 0.056 ms | 277.969 ± 33.595 ms † | **3.099 ± 0.123 ms** |
| medium/package-json | 74.378 ± 0.433 ms | — | — |
| medium/user-profile | 3.105 ± 0.120 ms | 3.919 ± 0.102 ms | **1.804 ± 0.078 ms** |
| small/config | 2.996 ± 0.045 ms | 3.877 ± 0.057 ms | **1.712 ± 0.036 ms** |
| small/prettierrc | 3.310 ± 0.148 ms | 3.925 ± 0.122 ms | **1.852 ± 0.054 ms** |

### validate (invalid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.98 ± 0.29 MiB | 10.03 MiB | **4.03 MiB** |
| generated/large-array | **55.83 ± 0.02 MiB** | 64.80 ± 0.00 MiB | 59.55 ± 0.01 MiB |
| generated/many-defs | 19.11 MiB | 21.44 ± 0.01 MiB | **13.00 MiB** |
| generated/wide-object | 15.08 ± 0.01 MiB | 15.41 MiB | **7.22 ± 0.01 MiB** |
| large/api-catalog | 5.91 ± 0.01 MiB | 10.48 MiB | **3.75 MiB** |
| large/circleci | 9.30 ± 0.11 MiB | 13.12 ± 0.03 MiB | **5.27 ± 0.02 MiB** |
| massive/data-warehouse | 22.50 ± 0.51 MiB | 21.72 ± 0.28 MiB | **9.91 ± 0.00 MiB** |
| massive/k8s-deployment | 11.81 MiB | 20.58 ± 0.04 MiB | **5.92 MiB** |
| medium/package-json | 27.67 ± 0.01 MiB | — | — |
| medium/user-profile | 5.73 ± 0.01 MiB | 10.20 ± 0.01 MiB | **3.62 MiB** |
| small/config | 5.75 ± 0.01 MiB | 10.00 ± 0.01 MiB | **3.61 MiB** |
| small/prettierrc | 5.86 ± 0.02 MiB | 9.12 ± 0.01 MiB | **3.72 ± 0.01 MiB** |

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
