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
| generated/deep-nesting | **378.534 ± 3.764 µs** | 9.715 ± 0.091 ms |
| generated/large-array | **45.151 ± 0.144 µs** | 1.042 ± 0.018 ms |
| generated/many-defs | **4.422 ± 0.073 ms** | 124.355 ± 4.039 ms |
| generated/wide-object | **5.667 ± 0.118 ms** | 151.231 ± 3.940 ms |
| large/api-catalog | **165.754 ± 2.010 µs** | 3.136 ± 0.057 ms |
| large/circleci | 1.694 ± 0.041 ms | — |
| massive/data-warehouse | **8.589 ± 0.182 ms** | 275.358 ± 5.305 ms |
| massive/k8s-deployment | 4.733 ± 0.118 ms | **1.816 ± 0.039 ms** |
| medium/package-json | 16.497 ± 0.354 ms | — |
| medium/user-profile | **61.850 ± 0.826 µs** | 1.447 ± 0.042 ms |
| small/config | **20.888 ± 0.312 µs** | 512.234 ± 5.075 µs |
| small/prettierrc | 326.914 ± 1.140 µs | — |

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
| generated/deep-nesting | 1.040 ± 0.018 ms | 208.202 ± 0.548 µs | **100.567 ± 0.972 µs** |
| generated/large-array | 44.789 ± 0.176 µs | 20.709 ± 0.386 µs | **10.724 ± 0.187 µs** |
| generated/many-defs | 14.807 ± 0.236 ms | 2.997 ± 0.133 ms | **1.637 ± 0.005 ms** |
| generated/wide-object | 23.407 ± 0.255 ms | 2.521 ± 0.019 ms | **1.296 ± 0.003 ms** |
| large/api-catalog | 177.888 ± 0.250 µs | 101.960 ± 0.809 µs | **43.467 ± 0.066 µs** |
| large/circleci | 2.874 ± 0.017 ms | 1.018 ± 0.007 ms | **509.198 ± 2.651 µs** |
| massive/data-warehouse | 45.402 ± 0.171 ms | 6.250 ± 0.022 ms | **2.602 ± 0.004 ms** |
| massive/k8s-deployment | 7.393 ± 0.058 ms | 1.592 ± 0.015 ms | **741.272 ± 1.210 µs** |
| medium/package-json | 68.822 ± 0.339 ms | 14.038 ± 0.098 ms | **5.824 ± 0.023 ms** |
| medium/user-profile | 66.932 ± 0.163 µs | 39.951 ± 0.256 µs | **16.277 ± 0.224 µs** |
| small/config | 27.203 ± 0.040 µs | 19.297 ± 0.270 µs | **7.527 ± 0.066 µs** |
| small/prettierrc | 207.844 ± 1.403 µs | 111.243 ± 0.275 µs | **56.263 ± 0.156 µs** |

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
| generated/deep-nesting | 6.424 ± 0.036 µs | **2.399 ± 0.021 µs** | 3.066 ± 0.055 µs |
| generated/large-array | 17.973 ± 0.083 ms | **7.384 ± 0.112 ms** | 8.723 ± 0.145 ms |
| generated/many-defs | 1.750 ± 0.007 ms | **616.486 ± 1.711 µs** | 750.875 ± 14.952 µs |
| generated/wide-object | 168.714 ± 1.612 µs | 108.344 ± 0.553 µs | **54.076 ± 0.746 µs** |
| large/api-catalog | 558.547 ± 2.074 ns | **240.434 ± 1.787 ns** | 421.126 ± 6.321 ns |
| large/circleci | 2.221 ± 0.029 µs | **467.309 ± 1.607 ns** | 989.872 ± 5.156 ns |
| massive/data-warehouse | 69.307 ± 0.971 µs | **23.632 ± 0.352 µs** | 44.926 ± 0.157 µs |
| massive/k8s-deployment | 515.080 ± 1.273 ns | **206.550 ± 0.352 ns** | 520.461 ± 10.951 ns |
| medium/package-json | 536.013 ± 0.881 ns | **226.600 ± 1.865 ns** | 485.213 ± 0.385 ns |
| medium/user-profile | 868.281 ± 38.738 ns | **389.396 ± 6.455 ns** | 498.468 ± 5.666 ns |
| small/config | 222.895 ± 1.356 ns | **111.749 ± 0.505 ns** | 144.507 ± 5.167 ns |
| small/prettierrc | 711.792 ± 3.481 ns | **173.764 ± 0.720 ns** | 278.683 ± 0.426 ns |

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
| generated/deep-nesting | 7.439 ± 0.016 µs | **2.774 ± 0.007 µs** | 5.203 ± 0.011 µs |
| generated/large-array | 17.862 ± 0.057 ms | **7.220 ± 0.061 ms** | 8.722 ± 0.123 ms |
| generated/many-defs | 1.746 ± 0.003 ms | **619.205 ± 1.758 µs** | 740.255 ± 1.817 µs |
| generated/wide-object | 169.007 ± 0.185 µs | 110.664 ± 0.876 µs | **54.232 ± 0.081 µs** |
| large/api-catalog | 645.517 ± 2.015 ns | **392.481 ± 1.455 ns** | 1.022 ± 0.004 µs |
| large/circleci | **125.152 ± 0.109 ns** | 129.808 ± 0.482 ns | 265.628 ± 1.244 ns |
| massive/data-warehouse | 180.652 ± 0.431 µs | **141.521 ± 0.390 µs** | 271.406 ± 1.400 µs |
| massive/k8s-deployment | 178.120 ± 0.435 ns | **134.602 ± 0.818 ns** | 242.379 ± 2.056 ns |
| medium/package-json | 171.618 ± 0.469 ns | **150.470 ± 0.466 ns** | 240.756 ± 0.892 ns |
| medium/user-profile | 363.407 ± 0.889 ns | **309.864 ± 1.068 ns** | 527.050 ± 2.907 ns |
| small/config | 284.371 ± 2.084 ns | **241.366 ± 0.607 ns** | 523.355 ± 3.300 ns |
| small/prettierrc | **527.363 ± 2.469 ns** | 918.394 ± 4.505 ns | 1.503 ± 0.004 µs |

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
| generated/deep-nesting | **3.047 ± 0.274 ms** | 107.128 ± 0.390 ms |
| generated/large-array | **1.811 ± 0.039 ms** | 86.447 ± 0.607 ms |
| generated/many-defs | **7.700 ± 0.276 ms** | 435.489 ± 3.620 ms |
| generated/wide-object | **9.032 ± 0.140 ms** | 510.893 ± 11.261 ms |
| large/api-catalog | **2.140 ± 0.061 ms** | 95.619 ± 1.691 ms |
| large/circleci | 4.466 ± 0.280 ms | — |
| massive/data-warehouse | **12.343 ± 0.066 ms** | 793.584 ± 4.168 ms |
| massive/k8s-deployment | **8.088 ± 0.085 ms** | 85.949 ± 0.722 ms |
| medium/package-json | 34.620 ± 0.250 ms | — |
| medium/user-profile | **1.946 ± 0.086 ms** | 90.830 ± 1.609 ms |
| small/config | **1.796 ± 0.039 ms** | 85.751 ± 1.579 ms |
| small/prettierrc | 2.324 ± 0.062 ms | — |

### codegen — peak RSS

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | **4.95 ± 0.01 MiB** | 38.02 MiB |
| generated/large-array | **3.78 ± 0.03 MiB** | 33.38 ± 0.01 MiB |
| generated/many-defs | **8.75 ± 0.03 MiB** | 98.09 MiB |
| generated/wide-object | **11.27 ± 0.07 MiB** | 115.34 ± 0.02 MiB |
| large/api-catalog | **4.25 MiB** | 34.64 MiB |
| large/circleci | 6.88 ± 0.08 MiB | — |
| massive/data-warehouse | **11.56 MiB** | 175.92 ± 0.19 MiB |
| massive/k8s-deployment | **8.78 ± 0.02 MiB** | 31.78 ± 0.01 MiB |
| medium/package-json | 19.69 MiB | — |
| medium/user-profile | **3.84 MiB** | 33.56 MiB |
| small/config | **3.64 ± 0.01 MiB** | 32.94 ± 0.01 MiB |
| small/prettierrc | 4.64 ± 0.01 MiB | — |

### validate (valid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.508 ± 0.449 ms | 4.083 ± 0.235 ms | **1.785 ± 0.033 ms** |
| generated/large-array | 59.169 ± 1.290 ms | 44.377 ± 0.432 ms | 44.154 ± 0.406 ms |
| generated/many-defs | 23.071 ± 0.303 ms | 11.430 ± 0.406 ms | **7.464 ± 0.868 ms †** |
| generated/wide-object | 27.893 ± 0.850 ms | 8.158 ± 0.120 ms | **3.890 ± 0.038 ms** |
| large/api-catalog | 3.179 ± 0.057 ms | 3.960 ± 0.108 ms | **1.787 ± 0.060 ms** |
| large/circleci | 6.928 ± 0.081 ms | 5.880 ± 0.455 ms | **2.599 ± 0.044 ms** |
| massive/data-warehouse | 49.617 ± 0.345 ms | 11.920 ± 0.106 ms | **5.376 ± 0.110 ms** |
| massive/k8s-deployment | 11.844 ± 0.174 ms | 309.193 ± 35.465 ms † | **2.901 ± 0.042 ms** |
| medium/package-json | 74.670 ± 0.174 ms | — | **7.559 ± 0.060 ms** |
| medium/user-profile | 3.089 ± 0.251 ms | 4.078 ± 0.126 ms | **1.676 ± 0.047 ms** |
| small/config | 2.906 ± 0.068 ms | 3.610 ± 0.054 ms | **1.582 ± 0.039 ms** |
| small/prettierrc | 3.095 ± 0.129 ms | 3.705 ± 0.127 ms | **1.685 ± 0.035 ms** |

### validate (valid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.97 MiB | 9.89 ± 0.01 MiB | **3.98 ± 0.01 MiB** |
| generated/large-array | **55.78 MiB** | 64.69 ± 0.02 MiB | 59.48 ± 0.01 MiB |
| generated/many-defs | 19.08 ± 0.03 MiB | 21.38 ± 0.03 MiB | **13.39 ± 0.21 MiB** |
| generated/wide-object | 15.08 ± 0.00 MiB | 15.28 ± 0.00 MiB | **7.20 ± 0.02 MiB** |
| large/api-catalog | 5.89 ± 0.01 MiB | 10.36 MiB | **3.78 MiB** |
| large/circleci | 9.06 ± 0.01 MiB | 13.05 ± 0.15 MiB | **5.39 MiB** |
| massive/data-warehouse | 22.05 ± 0.02 MiB | 20.98 ± 0.01 MiB | **9.33 ± 0.01 MiB** |
| massive/k8s-deployment | 11.80 ± 0.02 MiB | 20.45 ± 0.04 MiB | **5.98 MiB** |
| medium/package-json | 28.69 MiB | — | **11.44 ± 0.05 MiB** |
| medium/user-profile | 5.73 ± 0.02 MiB | 10.06 ± 0.02 MiB | **3.67 ± 0.01 MiB** |
| small/config | 5.72 MiB | 9.86 ± 0.01 MiB | **3.55 MiB** |
| small/prettierrc | 5.84 MiB | 8.97 ± 0.00 MiB | **3.72 ± 0.01 MiB** |

### validate (invalid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.119 ± 0.098 ms | 3.994 ± 0.114 ms | **1.829 ± 0.095 ms** |
| generated/large-array | 58.274 ± 0.428 ms | 44.171 ± 1.216 ms | 43.861 ± 0.441 ms |
| generated/many-defs | 22.860 ± 0.191 ms | 11.347 ± 0.099 ms | **7.362 ± 0.124 ms** |
| generated/wide-object | 27.821 ± 0.610 ms | 8.096 ± 0.063 ms | **3.944 ± 0.162 ms** |
| large/api-catalog | 3.153 ± 0.032 ms | 4.030 ± 0.119 ms | **1.766 ± 0.032 ms** |
| large/circleci | 7.067 ± 0.229 ms | 5.697 ± 0.052 ms | **2.583 ± 0.044 ms** |
| massive/data-warehouse | 50.876 ± 0.287 ms | 12.286 ± 1.274 ms † | **5.633 ± 0.222 ms** |
| massive/k8s-deployment | 11.873 ± 0.070 ms | 312.956 ± 51.965 ms † | **2.917 ± 0.064 ms** |
| medium/package-json | 74.572 ± 0.205 ms | — | **7.546 ± 0.076 ms** |
| medium/user-profile | 2.970 ± 0.062 ms | 3.977 ± 0.066 ms | **1.687 ± 0.039 ms** |
| small/config | 2.826 ± 0.030 ms | 3.719 ± 0.078 ms | **1.599 ± 0.030 ms** |
| small/prettierrc | 3.144 ± 0.141 ms | 3.686 ± 0.047 ms | **1.720 ± 0.083 ms** |

### validate (invalid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.97 MiB | 10.02 ± 0.02 MiB | **4.12 MiB** |
| generated/large-array | **55.80 ± 0.01 MiB** | 64.81 ± 0.02 MiB | 59.56 ± 0.01 MiB |
| generated/many-defs | 19.09 ± 0.01 MiB | 21.41 ± 0.01 MiB | **13.09 ± 0.01 MiB** |
| generated/wide-object | 15.12 ± 0.71 MiB | 15.39 MiB | **7.28 ± 0.02 MiB** |
| large/api-catalog | 5.94 ± 0.02 MiB | 10.50 MiB | **3.84 MiB** |
| large/circleci | 9.03 ± 0.03 MiB | 13.03 ± 0.02 MiB | **5.47 MiB** |
| massive/data-warehouse | 22.72 ± 0.08 MiB | 21.05 ± 0.31 MiB | **10.09 ± 0.28 MiB** |
| massive/k8s-deployment | 11.81 ± 0.02 MiB | 20.47 ± 0.03 MiB | **6.02 MiB** |
| medium/package-json | 27.66 ± 0.00 MiB | — | **11.45 MiB** |
| medium/user-profile | 5.73 ± 0.02 MiB | 10.22 ± 0.01 MiB | **3.70 ± 0.01 MiB** |
| small/config | 5.75 MiB | 10.03 MiB | **3.67 ± 0.00 MiB** |
| small/prettierrc | 5.86 ± 0.01 MiB | 9.16 MiB | **3.77 ± 0.01 MiB** |

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
