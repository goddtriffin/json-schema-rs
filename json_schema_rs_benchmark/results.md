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
| CLI | validate (valid) | time | jsonschemars | 11 / 11 |
| CLI | validate (valid) | peak RSS | jsonschemars | 10 / 11 |
| CLI | validate (invalid) | time | jsonschemars | 10 / 11 |
| CLI | validate (invalid) | peak RSS | jsonschemars | 10 / 11 |

## In-process (library calls)

### codegen — time

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | **375.322 ± 3.069 µs** | 9.752 ± 0.117 ms |
| generated/large-array | **45.022 ± 0.397 µs** | 1.032 ± 0.008 ms |
| generated/many-defs | **4.353 ± 0.019 ms** | 121.313 ± 1.042 ms |
| generated/wide-object | **5.615 ± 0.020 ms** | 147.847 ± 1.676 ms |
| large/api-catalog | **164.434 ± 0.379 µs** | 3.099 ± 0.042 ms |
| large/circleci | 1.682 ± 0.011 ms | — |
| massive/data-warehouse | **8.455 ± 0.062 ms** | 270.216 ± 0.586 ms |
| massive/k8s-deployment | 4.668 ± 0.017 ms | **1.806 ± 0.013 ms** |
| medium/package-json | 16.198 ± 0.070 ms | — |
| medium/user-profile | **60.964 ± 0.456 µs** | 1.412 ± 0.023 ms |
| small/config | **20.761 ± 0.102 µs** | 514.204 ± 1.422 µs |
| small/prettierrc | 324.656 ± 1.012 µs | — |

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
| generated/deep-nesting | 1.025 ± 0.002 ms | 209.390 ± 2.358 µs | **100.433 ± 0.264 µs** |
| generated/large-array | 44.264 ± 2.821 µs | 20.433 ± 0.161 µs | **10.602 ± 0.028 µs** |
| generated/many-defs | 13.678 ± 0.143 ms | 2.981 ± 0.035 ms | **1.689 ± 0.012 ms** |
| generated/wide-object | 23.093 ± 0.060 ms | 2.485 ± 0.024 ms | **1.306 ± 0.029 ms** |
| large/api-catalog | 177.386 ± 1.304 µs | 102.335 ± 0.176 µs | **43.384 ± 0.261 µs** |
| large/circleci | 2.846 ± 0.008 ms | 1.019 ± 0.003 ms | **507.270 ± 2.172 µs** |
| massive/data-warehouse | 43.809 ± 0.142 ms | 6.212 ± 0.020 ms | **2.608 ± 0.006 ms** |
| massive/k8s-deployment | 7.350 ± 0.022 ms | 1.579 ± 0.003 ms | **736.396 ± 1.705 µs** |
| medium/package-json | 67.994 ± 0.181 ms | 14.192 ± 0.157 ms | **5.823 ± 0.071 ms** |
| medium/user-profile | 66.254 ± 0.512 µs | 39.605 ± 0.477 µs | **16.398 ± 0.020 µs** |
| small/config | 26.792 ± 0.149 µs | 18.943 ± 0.061 µs | **7.415 ± 0.033 µs** |
| small/prettierrc | 202.730 ± 0.272 µs | 109.991 ± 0.665 µs | **56.289 ± 0.163 µs** |

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
| generated/deep-nesting | 6.743 ± 0.014 µs | **2.428 ± 0.008 µs** | 3.068 ± 0.055 µs |
| generated/large-array | 17.949 ± 0.110 ms | **7.230 ± 0.045 ms** | 8.651 ± 0.092 ms |
| generated/many-defs | 1.717 ± 0.003 ms | **624.992 ± 3.521 µs** | 733.904 ± 3.523 µs |
| generated/wide-object | 165.362 ± 1.653 µs | 114.496 ± 0.477 µs | **53.725 ± 0.240 µs** |
| large/api-catalog | 578.315 ± 10.052 ns | **244.070 ± 2.126 ns** | 423.442 ± 4.092 ns |
| large/circleci | 2.222 ± 0.009 µs | **469.858 ± 11.134 ns** | 1.005 ± 0.022 µs |
| massive/data-warehouse | 69.079 ± 0.313 µs | **23.975 ± 0.172 µs** | 44.609 ± 0.270 µs |
| massive/k8s-deployment | 519.906 ± 3.370 ns | **209.691 ± 0.846 ns** | 503.180 ± 1.873 ns |
| medium/package-json | 561.400 ± 6.757 ns | **231.111 ± 16.791 ns** | 487.464 ± 3.350 ns |
| medium/user-profile | 837.222 ± 1.646 ns | **376.163 ± 15.168 ns** | 515.044 ± 8.371 ns |
| small/config | 235.682 ± 5.592 ns | **115.066 ± 2.767 ns** | 149.416 ± 6.563 ns |
| small/prettierrc | 761.899 ± 4.913 ns | **182.761 ± 2.974 ns** | 302.444 ± 7.123 ns |

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
| generated/deep-nesting | 7.890 ± 0.076 µs | **2.884 ± 0.033 µs** | 5.375 ± 0.057 µs |
| generated/large-array | 18.305 ± 0.226 ms | **7.567 ± 0.143 ms** | 9.168 ± 0.245 ms |
| generated/many-defs | 1.712 ± 0.022 ms | **624.770 ± 1.194 µs** | 749.925 ± 3.082 µs |
| generated/wide-object | 164.376 ± 2.706 µs | 115.563 ± 2.664 µs | **54.448 ± 0.073 µs** |
| large/api-catalog | 656.162 ± 1.698 ns | **392.483 ± 1.031 ns** | 1.024 ± 0.003 µs |
| large/circleci | **124.504 ± 0.142 ns** | 131.229 ± 2.367 ns | 266.000 ± 0.681 ns |
| massive/data-warehouse | 179.605 ± 0.422 µs | **138.942 ± 0.490 µs** | 272.683 ± 1.676 µs |
| massive/k8s-deployment | 189.998 ± 1.483 ns | **141.337 ± 3.151 ns** | 242.388 ± 1.368 ns |
| medium/package-json | 184.912 ± 2.378 ns | **162.165 ± 2.203 ns** | 258.835 ± 3.552 ns |
| medium/user-profile | 377.547 ± 6.115 ns | **329.095 ± 6.303 ns** | 574.824 ± 15.443 ns |
| small/config | 289.003 ± 4.401 ns | **248.756 ± 0.873 ns** | 534.630 ± 6.075 ns |
| small/prettierrc | **535.823 ± 5.674 ns** | 923.754 ± 9.931 ns | 1.570 ± 0.017 µs |

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
| generated/deep-nesting | **2.698 ± 0.074 ms** | 114.407 ± 1.700 ms |
| generated/large-array | **2.068 ± 0.040 ms** | 92.818 ± 1.676 ms |
| generated/many-defs | **8.382 ± 0.192 ms** | 451.657 ± 3.278 ms |
| generated/wide-object | **9.528 ± 0.294 ms** | 532.790 ± 7.976 ms |
| large/api-catalog | **2.480 ± 0.128 ms** | 102.812 ± 1.820 ms |
| large/circleci | 4.848 ± 0.099 ms | — |
| massive/data-warehouse | **13.177 ± 0.457 ms** | 817.134 ± 15.796 ms |
| massive/k8s-deployment | **8.819 ± 0.084 ms** | 90.828 ± 1.811 ms |
| medium/package-json | 35.097 ± 0.324 ms | — |
| medium/user-profile | **2.028 ± 0.066 ms** | 91.780 ± 1.234 ms |
| small/config | **1.903 ± 0.054 ms** | 88.120 ± 1.181 ms |
| small/prettierrc | 2.543 ± 0.046 ms | — |

### codegen — peak RSS

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | **5.03 MiB** | 37.97 ± 0.01 MiB |
| generated/large-array | **3.75 MiB** | 33.31 ± 0.02 MiB |
| generated/many-defs | **8.78 ± 0.19 MiB** | 98.14 ± 0.05 MiB |
| generated/wide-object | **11.11 ± 0.03 MiB** | 115.50 ± 0.38 MiB |
| large/api-catalog | **4.27 ± 0.00 MiB** | 34.75 MiB |
| large/circleci | 6.66 ± 0.00 MiB | — |
| massive/data-warehouse | **11.47 ± 0.01 MiB** | 176.08 ± 0.06 MiB |
| massive/k8s-deployment | **8.83 ± 0.03 MiB** | 31.81 ± 0.01 MiB |
| medium/package-json | 19.77 ± 0.06 MiB | — |
| medium/user-profile | **3.86 ± 0.01 MiB** | 33.55 ± 0.02 MiB |
| small/config | **3.58 ± 0.00 MiB** | 32.91 ± 0.01 MiB |
| small/prettierrc | 4.69 ± 0.01 MiB | — |

### validate (valid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.487 ± 0.082 ms | 4.451 ± 0.104 ms | **2.039 ± 0.107 ms** |
| generated/large-array | 61.322 ± 0.974 ms | 46.292 ± 0.490 ms | **44.589 ± 1.119 ms** |
| generated/many-defs | 23.864 ± 0.600 ms | 12.130 ± 0.354 ms | **7.102 ± 0.119 ms** |
| generated/wide-object | 28.841 ± 0.742 ms | 9.359 ± 0.599 ms | **4.370 ± 0.078 ms** |
| large/api-catalog | 3.662 ± 0.144 ms | 4.179 ± 0.363 ms | **1.983 ± 0.101 ms** |
| large/circleci | 7.723 ± 0.447 ms | 6.410 ± 0.537 ms | **2.820 ± 0.084 ms** |
| massive/data-warehouse | 52.605 ± 1.040 ms | 12.763 ± 0.480 ms | **5.892 ± 5.275 ms †** |
| massive/k8s-deployment | 11.957 ± 0.092 ms | 262.756 ± 39.505 ms † | **3.042 ± 0.087 ms** |
| medium/package-json | 74.597 ± 0.344 ms | — | — |
| medium/user-profile | 3.191 ± 0.114 ms | 3.912 ± 0.093 ms | **1.753 ± 0.063 ms** |
| small/config | 3.002 ± 0.095 ms | 3.855 ± 0.333 ms | **1.728 ± 0.051 ms** |
| small/prettierrc | 3.218 ± 0.077 ms | 3.870 ± 0.092 ms | **1.811 ± 0.058 ms** |

### validate (valid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.97 ± 0.01 MiB | 9.91 MiB | **3.92 MiB** |
| generated/large-array | **55.81 MiB** | 64.67 ± 0.01 MiB | 59.42 MiB |
| generated/many-defs | 19.08 ± 0.02 MiB | 21.33 ± 0.02 MiB | **12.84 ± 0.02 MiB** |
| generated/wide-object | 15.11 ± 0.01 MiB | 15.28 ± 0.01 MiB | **7.16 ± 0.01 MiB** |
| large/api-catalog | 5.89 MiB | 10.36 ± 0.02 MiB | **3.69 ± 0.01 MiB** |
| large/circleci | 9.09 ± 0.17 MiB | 13.03 ± 0.03 MiB | **5.22 MiB** |
| massive/data-warehouse | 22.03 ± 0.72 MiB | 20.98 MiB | **9.20 MiB** |
| massive/k8s-deployment | 11.78 MiB | 20.61 MiB | **5.83 ± 0.02 MiB** |
| medium/package-json | 27.67 MiB | — | — |
| medium/user-profile | 5.72 ± 0.01 MiB | 10.09 ± 0.01 MiB | **3.58 MiB** |
| small/config | 5.72 MiB | 9.86 ± 0.01 MiB | **3.50 MiB** |
| small/prettierrc | 5.84 ± 0.01 MiB | 8.94 ± 0.01 MiB | **3.70 ± 0.01 MiB** |

### validate (invalid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.671 ± 0.182 ms | 4.486 ± 0.108 ms | **1.981 ± 0.066 ms** |
| generated/large-array | 61.091 ± 0.778 ms | 46.521 ± 0.846 ms | 45.654 ± 0.434 ms |
| generated/many-defs | 26.791 ± 0.320 ms | 11.845 ± 0.124 ms | **7.212 ± 0.072 ms** |
| generated/wide-object | 29.881 ± 0.434 ms | 8.952 ± 0.200 ms | **4.485 ± 0.125 ms** |
| large/api-catalog | 3.671 ± 0.126 ms | 4.436 ± 0.296 ms | **1.844 ± 0.045 ms** |
| large/circleci | 7.471 ± 0.117 ms | 6.241 ± 0.259 ms | **2.805 ± 0.088 ms** |
| massive/data-warehouse | 54.368 ± 0.924 ms | 12.675 ± 0.140 ms | **6.010 ± 0.055 ms** |
| massive/k8s-deployment | 11.949 ± 0.062 ms | 308.150 ± 48.379 ms † | **3.032 ± 0.182 ms** |
| medium/package-json | 74.580 ± 1.192 ms | — | — |
| medium/user-profile | 3.071 ± 0.176 ms | 3.908 ± 0.356 ms | **1.775 ± 0.055 ms** |
| small/config | 3.007 ± 0.042 ms | 3.801 ± 0.089 ms | **1.732 ± 0.035 ms** |
| small/prettierrc | 3.269 ± 0.231 ms | 3.941 ± 0.330 ms | **1.779 ± 0.040 ms** |

### validate (invalid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.98 ± 0.02 MiB | 10.02 ± 0.02 MiB | **4.08 ± 0.01 MiB** |
| generated/large-array | **55.84 MiB** | 64.81 ± 0.00 MiB | 59.53 ± 0.01 MiB |
| generated/many-defs | 19.08 ± 0.00 MiB | 21.42 MiB | **12.94 MiB** |
| generated/wide-object | 15.09 ± 0.02 MiB | 15.42 ± 0.02 MiB | **7.28 MiB** |
| large/api-catalog | 5.89 ± 0.01 MiB | 10.50 ± 0.01 MiB | **3.80 MiB** |
| large/circleci | 9.05 MiB | 13.03 ± 0.02 MiB | **5.33 MiB** |
| massive/data-warehouse | 24.34 ± 0.02 MiB | 21.11 MiB | **9.47 MiB** |
| massive/k8s-deployment | 11.81 MiB | 20.56 ± 0.07 MiB | **5.92 ± 0.01 MiB** |
| medium/package-json | 27.67 ± 0.28 MiB | — | — |
| medium/user-profile | 5.72 ± 0.01 MiB | 10.20 MiB | **3.66 ± 0.01 MiB** |
| small/config | 5.73 ± 0.01 MiB | 10.02 ± 0.01 MiB | **3.66 MiB** |
| small/prettierrc | 5.86 MiB | 9.12 ± 0.01 MiB | **3.78 ± 0.01 MiB** |

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
