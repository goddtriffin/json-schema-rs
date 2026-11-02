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
| generated/deep-nesting | **374.073 ± 2.383 µs** | 9.604 ± 0.088 ms |
| generated/large-array | **44.976 ± 0.142 µs** | 1.026 ± 0.002 ms |
| generated/many-defs | **4.403 ± 0.048 ms** | 121.147 ± 0.773 ms |
| generated/wide-object | **5.625 ± 0.050 ms** | 147.216 ± 0.894 ms |
| large/api-catalog | **163.898 ± 0.398 µs** | 3.086 ± 0.010 ms |
| large/circleci | 1.676 ± 0.008 ms | — |
| massive/data-warehouse | **8.451 ± 0.082 ms** | 268.940 ± 4.087 ms |
| massive/k8s-deployment | 4.767 ± 0.004 ms | **1.800 ± 0.001 ms** |
| medium/package-json | 16.079 ± 0.022 ms | — |
| medium/user-profile | **60.719 ± 0.043 µs** | 1.419 ± 0.020 ms |
| small/config | **20.740 ± 0.030 µs** | 507.053 ± 0.718 µs |
| small/prettierrc | 323.508 ± 2.244 µs | — |

### codegen — peak heap

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | **348.22 KiB** | 12.65 MiB |
| generated/large-array | **31.94 KiB** | 1.23 MiB |
| generated/many-defs | **3.36 MiB** | 133.38 MiB |
| generated/wide-object | **4.74 MiB** | 162.36 MiB |
| large/api-catalog | **99.49 KiB** | 3.64 MiB |
| large/circleci | 1.57 MiB | — |
| massive/data-warehouse | **5.81 MiB** | 288.90 MiB |
| massive/k8s-deployment | 3.49 MiB | **2.79 MiB** |
| medium/package-json | 15.73 MiB | — |
| medium/user-profile | **43.08 KiB** | 1.71 MiB |
| small/config | **16.63 KiB** | 639.58 KiB |
| small/prettierrc | 183.78 KiB | — |

### schema compile — time

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 1.019 ± 0.002 ms | 205.864 ± 0.214 µs | **99.833 ± 0.315 µs** |
| generated/large-array | 44.231 ± 0.041 µs | 20.452 ± 0.019 µs | **10.549 ± 0.015 µs** |
| generated/many-defs | 13.691 ± 0.013 ms | 2.978 ± 0.002 ms | **1.630 ± 0.012 ms** |
| generated/wide-object | 23.143 ± 0.113 ms | 2.481 ± 0.002 ms | **1.292 ± 0.002 ms** |
| large/api-catalog | 176.751 ± 0.140 µs | 101.579 ± 0.164 µs | **43.738 ± 0.067 µs** |
| large/circleci | 2.844 ± 0.009 ms | 1.011 ± 0.001 ms | **509.815 ± 2.333 µs** |
| massive/data-warehouse | 44.090 ± 0.129 ms | 6.274 ± 0.060 ms | **2.604 ± 0.002 ms** |
| massive/k8s-deployment | 7.335 ± 0.008 ms | 1.592 ± 0.001 ms | **817.905 ± 1.671 µs** |
| medium/package-json | 68.596 ± 0.156 ms | 13.943 ± 0.090 ms | **5.787 ± 0.035 ms** |
| medium/user-profile | 66.213 ± 0.057 µs | 39.363 ± 0.032 µs | **16.466 ± 0.016 µs** |
| small/config | 26.583 ± 0.117 µs | 18.839 ± 0.019 µs | **7.436 ± 0.012 µs** |
| small/prettierrc | 202.716 ± 0.217 µs | 109.010 ± 0.125 µs | **56.351 ± 0.139 µs** |

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
| generated/deep-nesting | 6.675 ± 0.007 µs | **2.525 ± 0.005 µs** | 3.042 ± 0.016 µs |
| generated/large-array | 17.984 ± 0.094 ms | **7.448 ± 0.015 ms** | 9.715 ± 0.045 ms |
| generated/many-defs | 1.759 ± 0.010 ms | **622.561 ± 2.620 µs** | 772.686 ± 2.023 µs |
| generated/wide-object | 172.447 ± 0.479 µs | 111.547 ± 1.777 µs | **55.632 ± 0.059 µs** |
| large/api-catalog | 559.269 ± 1.412 ns | **238.452 ± 1.469 ns** | 427.282 ± 3.367 ns |
| large/circleci | 2.230 ± 0.019 µs | **476.386 ± 2.526 ns** | 1.011 ± 0.008 µs |
| massive/data-warehouse | 69.414 ± 0.210 µs | **24.364 ± 0.186 µs** | 45.814 ± 0.806 µs |
| massive/k8s-deployment | 517.138 ± 5.514 ns | **215.641 ± 1.092 ns** | 495.895 ± 0.971 ns |
| medium/package-json | 535.082 ± 3.838 ns | **226.952 ± 0.791 ns** | 484.646 ± 0.424 ns |
| medium/user-profile | 834.124 ± 11.367 ns | **384.989 ± 3.917 ns** | 493.049 ± 5.720 ns |
| small/config | 222.906 ± 0.367 ns | **116.452 ± 0.438 ns** | 150.704 ± 2.669 ns |
| small/prettierrc | 724.107 ± 2.925 ns | **176.269 ± 0.922 ns** | 284.730 ± 2.018 ns |

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
| generated/deep-nesting | 7.571 ± 0.023 µs | **2.878 ± 0.013 µs** | 5.250 ± 0.057 µs |
| generated/large-array | 17.891 ± 0.035 ms | **7.500 ± 0.046 ms** | 9.798 ± 0.043 ms |
| generated/many-defs | 1.749 ± 0.002 ms | **625.787 ± 1.544 µs** | 759.489 ± 4.004 µs |
| generated/wide-object | 172.220 ± 1.259 µs | 110.072 ± 0.238 µs | **56.002 ± 0.185 µs** |
| large/api-catalog | 655.630 ± 1.698 ns | **398.774 ± 3.793 ns** | 1.038 ± 0.009 µs |
| large/circleci | 129.228 ± 1.980 ns | 131.563 ± 0.257 ns | 267.676 ± 1.330 ns |
| massive/data-warehouse | 179.004 ± 0.510 µs | **138.486 ± 0.215 µs** | 274.912 ± 1.870 µs |
| massive/k8s-deployment | 176.904 ± 0.268 ns | **136.923 ± 1.462 ns** | 248.866 ± 3.381 ns |
| medium/package-json | 173.302 ± 0.237 ns | **152.329 ± 0.445 ns** | 241.966 ± 0.293 ns |
| medium/user-profile | 364.648 ± 2.027 ns | **319.574 ± 1.081 ns** | 531.339 ± 3.548 ns |
| small/config | 289.191 ± 1.976 ns | **248.749 ± 0.579 ns** | 533.435 ± 2.541 ns |
| small/prettierrc | **524.334 ± 0.731 ns** | 923.160 ± 6.995 ns | 1.517 ± 0.013 µs |

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
| generated/deep-nesting | **2.954 ± 0.316 ms †** | 106.616 ± 0.779 ms |
| generated/large-array | **1.867 ± 0.038 ms** | 85.860 ± 0.573 ms |
| generated/many-defs | **7.772 ± 0.072 ms** | 433.056 ± 1.374 ms |
| generated/wide-object | **8.949 ± 0.080 ms** | 505.403 ± 2.329 ms |
| large/api-catalog | **2.171 ± 0.039 ms** | 93.512 ± 0.416 ms |
| large/circleci | 4.403 ± 0.044 ms | — |
| massive/data-warehouse | **12.493 ± 0.055 ms** | 783.741 ± 4.086 ms |
| massive/k8s-deployment | **8.183 ± 0.061 ms** | 85.751 ± 0.391 ms |
| medium/package-json | 34.576 ± 0.242 ms | — |
| medium/user-profile | **1.874 ± 0.054 ms** | 88.038 ± 1.334 ms |
| small/config | **1.765 ± 0.049 ms** | 85.119 ± 0.475 ms |
| small/prettierrc | 2.340 ± 0.049 ms | — |

### codegen — peak RSS

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | **5.20 ± 0.01 MiB** | 37.98 ± 0.01 MiB |
| generated/large-array | **3.95 MiB** | 33.31 MiB |
| generated/many-defs | **9.00 ± 0.02 MiB** | 98.09 ± 0.05 MiB |
| generated/wide-object | **11.44 ± 0.01 MiB** | 113.86 ± 0.47 MiB |
| large/api-catalog | **4.42 ± 0.01 MiB** | 34.70 MiB |
| large/circleci | 6.98 ± 0.01 MiB | — |
| massive/data-warehouse | **12.30 MiB** | 175.89 ± 0.05 MiB |
| massive/k8s-deployment | **9.48 MiB** | 31.81 MiB |
| medium/package-json | 19.86 ± 0.02 MiB | — |
| medium/user-profile | **4.08 ± 0.02 MiB** | 33.58 ± 0.01 MiB |
| small/config | **3.77 ± 0.00 MiB** | 32.94 MiB |
| small/prettierrc | 4.78 ± 0.01 MiB | — |

### validate (valid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.454 ± 0.402 ms | 4.073 ± 0.068 ms | **1.792 ± 0.033 ms** |
| generated/large-array | 58.080 ± 0.421 ms | 44.368 ± 0.864 ms | 43.865 ± 0.357 ms |
| generated/many-defs | 23.044 ± 0.325 ms | 11.343 ± 0.089 ms | **7.317 ± 0.075 ms** |
| generated/wide-object | 27.789 ± 0.254 ms | 8.067 ± 0.117 ms | **3.831 ± 0.051 ms** |
| large/api-catalog | 3.098 ± 0.044 ms | 3.935 ± 0.098 ms | **1.755 ± 0.042 ms** |
| large/circleci | 6.831 ± 0.141 ms | 5.785 ± 0.231 ms | **2.538 ± 0.041 ms** |
| massive/data-warehouse | 49.619 ± 0.347 ms | 11.852 ± 0.055 ms | **5.296 ± 0.037 ms** |
| massive/k8s-deployment | 11.787 ± 0.095 ms | 277.428 ± 41.905 ms † | **2.872 ± 0.029 ms** |
| medium/package-json | 74.353 ± 0.148 ms | — | **7.563 ± 0.141 ms** |
| medium/user-profile | 2.957 ± 0.180 ms | 3.824 ± 0.282 ms | **1.645 ± 0.041 ms** |
| small/config | 2.872 ± 0.030 ms | 3.650 ± 0.218 ms | **1.602 ± 0.030 ms** |
| small/prettierrc | 3.073 ± 0.033 ms | 3.722 ± 0.147 ms | **1.705 ± 0.025 ms** |

### validate (valid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.97 MiB | 9.91 ± 0.01 MiB | **4.09 ± 0.01 MiB** |
| generated/large-array | **55.77 MiB** | 64.70 ± 0.02 MiB | 59.59 ± 0.01 MiB |
| generated/many-defs | 19.08 MiB | 21.38 ± 0.01 MiB | **13.11 ± 0.01 MiB** |
| generated/wide-object | 15.08 ± 0.01 MiB | 15.30 ± 0.01 MiB | **7.25 MiB** |
| large/api-catalog | 5.89 MiB | 10.36 ± 0.01 MiB | **3.83 MiB** |
| large/circleci | 9.06 ± 0.01 MiB | 13.03 MiB | **5.36 ± 0.03 MiB** |
| massive/data-warehouse | 22.05 ± 0.00 MiB | 20.97 MiB | **9.39 ± 0.01 MiB** |
| massive/k8s-deployment | 11.78 MiB | 20.55 ± 0.02 MiB | **5.98 MiB** |
| medium/package-json | 27.67 ± 0.03 MiB | — | **11.48 ± 0.02 MiB** |
| medium/user-profile | 5.72 ± 0.01 MiB | 10.08 ± 0.01 MiB | **3.69 ± 0.01 MiB** |
| small/config | 5.72 ± 0.01 MiB | 9.86 ± 0.01 MiB | **3.59 MiB** |
| small/prettierrc | 5.84 MiB | 8.97 MiB | **3.78 MiB** |

### validate (invalid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.166 ± 0.150 ms | 4.079 ± 0.088 ms | **1.807 ± 0.029 ms** |
| generated/large-array | 57.903 ± 0.278 ms | 44.174 ± 0.890 ms | 43.867 ± 1.148 ms |
| generated/many-defs | 23.012 ± 0.316 ms | 11.315 ± 0.100 ms | **7.304 ± 0.041 ms** |
| generated/wide-object | 28.180 ± 0.273 ms | 8.024 ± 0.077 ms | **3.853 ± 0.044 ms** |
| large/api-catalog | 3.109 ± 0.039 ms | 3.905 ± 0.068 ms | **1.746 ± 0.030 ms** |
| large/circleci | 6.818 ± 0.109 ms | 5.748 ± 0.067 ms | **2.541 ± 0.022 ms** |
| massive/data-warehouse | 50.890 ± 1.210 ms | 12.018 ± 0.147 ms | **5.435 ± 0.056 ms** |
| massive/k8s-deployment | 11.815 ± 0.081 ms | 270.578 ± 36.663 ms † | **2.884 ± 0.047 ms** |
| medium/package-json | 74.358 ± 0.312 ms | — | **7.534 ± 0.053 ms** |
| medium/user-profile | 2.971 ± 0.238 ms | 3.706 ± 0.102 ms | **1.622 ± 0.028 ms** |
| small/config | 2.871 ± 0.041 ms | 3.635 ± 0.043 ms | **1.597 ± 0.029 ms** |
| small/prettierrc | 3.082 ± 0.034 ms | 3.841 ± 0.145 ms | **1.683 ± 0.024 ms** |

### validate (invalid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.98 ± 0.02 MiB | 10.02 ± 0.02 MiB | **4.16 MiB** |
| generated/large-array | **55.81 ± 0.00 MiB** | 64.80 ± 0.01 MiB | 59.61 MiB |
| generated/many-defs | 19.11 ± 0.01 MiB | 21.42 MiB | **13.12 ± 0.01 MiB** |
| generated/wide-object | 15.12 MiB | 15.41 ± 0.01 MiB | **7.30 ± 0.01 MiB** |
| large/api-catalog | 5.89 MiB | 10.50 ± 0.02 MiB | **3.88 MiB** |
| large/circleci | 9.06 ± 0.01 MiB | 13.09 ± 0.07 MiB | **5.44 ± 0.02 MiB** |
| massive/data-warehouse | 22.58 ± 0.04 MiB | 21.12 ± 0.05 MiB | **9.55 ± 0.01 MiB** |
| massive/k8s-deployment | 11.81 ± 0.02 MiB | 20.52 ± 0.04 MiB | **5.98 MiB** |
| medium/package-json | 27.67 ± 0.04 MiB | — | **11.58 ± 0.04 MiB** |
| medium/user-profile | 5.72 ± 0.01 MiB | 10.22 ± 0.01 MiB | **3.69 MiB** |
| small/config | 5.72 ± 0.01 MiB | 10.02 MiB | **3.69 MiB** |
| small/prettierrc | 5.86 ± 0.01 MiB | 9.11 ± 0.01 MiB | **3.83 ± 0.01 MiB** |

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
