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
| generated/deep-nesting | **3.031 ± 0.002 ms** | 9.582 ± 0.009 ms |
| generated/large-array | **132.124 ± 0.218 µs** | 1.022 ± 0.001 ms |
| generated/many-defs | **19.996 ± 0.140 ms** | 120.697 ± 2.594 ms |
| generated/wide-object | **17.450 ± 0.024 ms** | 146.348 ± 0.452 ms |
| large/api-catalog | **455.346 ± 1.128 µs** | 3.071 ± 0.004 ms |
| large/circleci | 6.801 ± 0.018 ms | — |
| massive/data-warehouse | **36.072 ± 0.321 ms** | 267.769 ± 1.785 ms |
| massive/k8s-deployment | 15.421 ± 0.025 ms | **1.789 ± 0.003 ms** |
| medium/package-json | 43.403 ± 0.290 ms | — |
| medium/user-profile | **183.996 ± 0.224 µs** | 1.410 ± 0.023 ms |
| small/config | **63.602 ± 0.076 µs** | 505.951 ± 0.535 µs |
| small/prettierrc | 846.582 ± 0.832 µs | — |

### codegen — peak heap

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | **1.39 MiB** | 12.65 MiB |
| generated/large-array | **81.24 KiB** | 1.23 MiB |
| generated/many-defs | **10.85 MiB** | 133.38 MiB |
| generated/wide-object | **10.44 MiB** | 162.36 MiB |
| large/api-catalog | **285.27 KiB** | 3.64 MiB |
| large/circleci | 3.90 MiB | — |
| massive/data-warehouse | **16.82 MiB** | 288.90 MiB |
| massive/k8s-deployment | 8.42 MiB | **2.79 MiB** |
| medium/package-json | 32.55 MiB | — |
| medium/user-profile | **116.96 KiB** | 1.71 MiB |
| small/config | **40.64 KiB** | 639.58 KiB |
| small/prettierrc | 528.44 KiB | — |

### schema compile — time

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 1.010 ± 0.003 ms | 207.086 ± 0.719 µs | **99.579 ± 0.197 µs** |
| generated/large-array | 44.526 ± 0.069 µs | 20.583 ± 0.034 µs | **10.655 ± 0.018 µs** |
| generated/many-defs | 13.795 ± 0.024 ms | 2.996 ± 0.005 ms | **1.615 ± 0.003 ms** |
| generated/wide-object | 23.270 ± 0.139 ms | 2.500 ± 0.014 ms | **1.294 ± 0.002 ms** |
| large/api-catalog | 176.300 ± 0.293 µs | 102.833 ± 0.178 µs | **43.456 ± 0.036 µs** |
| large/circleci | 2.873 ± 0.028 ms | 1.013 ± 0.006 ms | **507.024 ± 1.160 µs** |
| massive/data-warehouse | 45.461 ± 0.299 ms | 6.291 ± 0.013 ms | **2.629 ± 0.004 ms** |
| massive/k8s-deployment | 7.305 ± 0.008 ms | 1.589 ± 0.008 ms | **737.560 ± 0.715 µs** |
| medium/package-json | 68.415 ± 0.108 ms | 13.988 ± 0.040 ms | **5.824 ± 0.113 ms** |
| medium/user-profile | 66.101 ± 0.041 µs | 39.444 ± 0.076 µs | **16.397 ± 0.042 µs** |
| small/config | 26.585 ± 0.189 µs | 18.890 ± 0.087 µs | **7.410 ± 0.025 µs** |
| small/prettierrc | 203.257 ± 1.183 µs | 109.222 ± 0.253 µs | **56.185 ± 0.101 µs** |

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
| generated/deep-nesting | 6.741 ± 0.004 µs | **2.467 ± 0.049 µs** | 3.047 ± 0.039 µs |
| generated/large-array | 17.780 ± 0.166 ms | **7.604 ± 0.083 ms** | 8.533 ± 0.107 ms |
| generated/many-defs | 1.718 ± 0.003 ms | **621.634 ± 1.539 µs** | 738.451 ± 5.963 µs |
| generated/wide-object | 166.602 ± 0.272 µs | 113.836 ± 0.438 µs | **53.117 ± 0.105 µs** |
| large/api-catalog | 570.453 ± 1.726 ns | **241.788 ± 1.603 ns** | 418.847 ± 2.778 ns |
| large/circleci | 2.221 ± 0.012 µs | **471.852 ± 1.235 ns** | 1.000 ± 0.009 µs |
| massive/data-warehouse | 69.519 ± 0.249 µs | **24.115 ± 0.111 µs** | 44.687 ± 0.386 µs |
| massive/k8s-deployment | 517.552 ± 0.675 ns | **215.633 ± 0.768 ns** | 496.480 ± 2.281 ns |
| medium/package-json | 535.999 ± 2.662 ns | **232.563 ± 4.231 ns** | 483.983 ± 0.721 ns |
| medium/user-profile | 838.641 ± 1.087 ns | **378.711 ± 2.597 ns** | 500.293 ± 5.711 ns |
| small/config | 222.742 ± 0.399 ns | **115.103 ± 0.460 ns** | 138.994 ± 2.518 ns |
| small/prettierrc | 724.322 ± 2.071 ns | **177.376 ± 1.676 ns** | 279.446 ± 1.008 ns |

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
| generated/deep-nesting | 7.719 ± 0.010 µs | **2.858 ± 0.012 µs** | 5.180 ± 0.030 µs |
| generated/large-array | 17.794 ± 0.097 ms | **7.572 ± 0.114 ms** | 8.582 ± 0.118 ms |
| generated/many-defs | 1.713 ± 0.008 ms | **623.516 ± 1.180 µs** | 741.526 ± 5.682 µs |
| generated/wide-object | 168.889 ± 0.537 µs | 110.625 ± 1.575 µs | **54.585 ± 0.157 µs** |
| large/api-catalog | 656.434 ± 1.429 ns | **398.501 ± 1.335 ns** | 1.028 ± 0.009 µs |
| large/circleci | 125.873 ± 0.117 ns | 128.642 ± 1.654 ns | 268.955 ± 1.471 ns |
| massive/data-warehouse | 180.274 ± 0.898 µs | **137.363 ± 0.587 µs** | 273.268 ± 1.184 µs |
| massive/k8s-deployment | 179.533 ± 0.153 ns | **134.917 ± 0.708 ns** | 245.575 ± 0.774 ns |
| medium/package-json | 173.755 ± 0.370 ns | **150.328 ± 0.799 ns** | 242.574 ± 1.528 ns |
| medium/user-profile | 369.556 ± 1.107 ns | **314.702 ± 2.093 ns** | 546.262 ± 2.317 ns |
| small/config | 293.312 ± 3.597 ns | **248.130 ± 0.680 ns** | 534.733 ± 1.994 ns |
| small/prettierrc | **527.645 ± 2.757 ns** | 926.029 ± 5.415 ns | 1.519 ± 0.005 µs |

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
| generated/deep-nesting | **5.975 ± 0.525 ms** | 108.228 ± 0.423 ms |
| generated/large-array | **2.077 ± 0.043 ms** | 87.195 ± 0.293 ms |
| generated/many-defs | **27.289 ± 0.174 ms** | 435.539 ± 1.765 ms |
| generated/wide-object | **24.212 ± 0.080 ms** | 506.097 ± 1.572 ms |
| large/api-catalog | **2.618 ± 0.038 ms** | 93.864 ± 0.640 ms |
| large/circleci | 10.836 ± 0.054 ms | — |
| massive/data-warehouse | **42.912 ± 0.140 ms** | 785.417 ± 2.372 ms |
| massive/k8s-deployment | **21.006 ± 0.173 ms** | 87.038 ± 2.469 ms |
| medium/package-json | 95.584 ± 0.288 ms | — |
| medium/user-profile | **2.185 ± 0.026 ms** | 86.848 ± 0.762 ms |
| small/config | **1.929 ± 0.036 ms** | 84.055 ± 1.070 ms |
| small/prettierrc | 3.103 ± 0.038 ms | — |

### codegen — peak RSS

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | **7.56 MiB** | 37.98 MiB |
| generated/large-array | **4.62 ± 0.01 MiB** | 33.31 ± 0.02 MiB |
| generated/many-defs | **21.92 ± 0.02 MiB** | 98.00 ± 0.01 MiB |
| generated/wide-object | **22.84 ± 0.03 MiB** | 114.81 ± 0.43 MiB |
| large/api-catalog | **5.27 ± 0.03 MiB** | 34.70 ± 0.02 MiB |
| large/circleci | 10.91 ± 0.01 MiB | — |
| massive/data-warehouse | **31.42 MiB** | 176.17 MiB |
| massive/k8s-deployment | **17.53 MiB** | 31.80 MiB |
| medium/package-json | 37.59 ± 0.08 MiB | — |
| medium/user-profile | **4.83 MiB** | 33.59 MiB |
| small/config | **4.33 MiB** | 32.94 MiB |
| small/prettierrc | 5.62 ± 0.05 MiB | — |

### validate (valid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.482 ± 0.429 ms | 4.011 ± 0.054 ms | **1.796 ± 0.033 ms** |
| generated/large-array | 58.296 ± 0.738 ms | 44.997 ± 6.533 ms † | 43.919 ± 0.415 ms |
| generated/many-defs | 22.912 ± 0.203 ms | 11.301 ± 0.074 ms | **7.337 ± 0.065 ms** |
| generated/wide-object | 27.702 ± 0.139 ms | 7.933 ± 0.048 ms | **3.802 ± 0.050 ms** |
| large/api-catalog | 3.095 ± 0.047 ms | 3.770 ± 0.054 ms | **1.690 ± 0.033 ms** |
| large/circleci | 6.734 ± 0.040 ms | 5.577 ± 0.095 ms | **2.473 ± 0.034 ms** |
| massive/data-warehouse | 49.514 ± 0.373 ms | 11.797 ± 0.232 ms | **5.284 ± 0.054 ms** |
| massive/k8s-deployment | 11.500 ± 0.129 ms | 329.841 ± 1092.031 ms † | **2.834 ± 0.041 ms** |
| medium/package-json | 73.901 ± 0.313 ms | — | **7.319 ± 0.095 ms** |
| medium/user-profile | 2.840 ± 0.031 ms | 3.606 ± 0.038 ms | **1.639 ± 0.037 ms** |
| small/config | 2.767 ± 0.022 ms | 3.557 ± 0.035 ms | **1.584 ± 0.030 ms** |
| small/prettierrc | 3.003 ± 0.037 ms | 3.604 ± 0.069 ms | **1.705 ± 0.039 ms** |

### validate (valid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.97 MiB | 9.89 ± 0.01 MiB | **4.06 MiB** |
| generated/large-array | **55.78 ± 0.01 MiB** | 64.70 MiB | 59.59 ± 0.01 MiB |
| generated/many-defs | 19.08 MiB | 21.38 MiB | **12.98 ± 0.02 MiB** |
| generated/wide-object | 15.11 ± 0.00 MiB | 15.28 ± 0.01 MiB | **7.28 MiB** |
| large/api-catalog | 5.89 MiB | 10.34 ± 0.01 MiB | **3.92 ± 0.01 MiB** |
| large/circleci | 9.08 MiB | 13.09 ± 0.03 MiB | **5.38 ± 0.01 MiB** |
| massive/data-warehouse | 22.03 MiB | 20.98 ± 0.01 MiB | **9.34 ± 0.01 MiB** |
| massive/k8s-deployment | 11.80 MiB | 20.34 ± 0.06 MiB | **6.00 MiB** |
| medium/package-json | 27.67 ± 0.02 MiB | — | **11.50 ± 0.01 MiB** |
| medium/user-profile | 5.75 MiB | 10.06 MiB | **3.72 ± 0.00 MiB** |
| small/config | 5.72 ± 0.01 MiB | 9.86 ± 0.02 MiB | **3.61 ± 0.01 MiB** |
| small/prettierrc | 5.84 ± 0.01 MiB | 8.95 MiB | **3.83 ± 0.01 MiB** |

### validate (invalid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.067 ± 0.041 ms | 3.990 ± 0.051 ms | **1.777 ± 0.051 ms** |
| generated/large-array | 58.304 ± 0.301 ms | 44.650 ± 0.348 ms | 43.756 ± 0.325 ms |
| generated/many-defs | 23.021 ± 0.248 ms | 11.357 ± 0.111 ms | **7.280 ± 0.051 ms** |
| generated/wide-object | 27.848 ± 0.197 ms | 7.931 ± 0.087 ms | **3.768 ± 0.042 ms** |
| large/api-catalog | 3.081 ± 0.042 ms | 3.802 ± 0.111 ms | **1.714 ± 0.020 ms** |
| large/circleci | 6.729 ± 0.053 ms | 5.595 ± 0.051 ms | **2.489 ± 0.027 ms** |
| massive/data-warehouse | 50.658 ± 1.250 ms | 11.683 ± 0.054 ms | **5.460 ± 0.060 ms** |
| massive/k8s-deployment | 11.481 ± 0.053 ms | 480.754 ± 1047.142 ms † | **2.839 ± 0.031 ms** |
| medium/package-json | 74.063 ± 0.262 ms | — | **7.222 ± 0.040 ms** |
| medium/user-profile | 2.831 ± 0.030 ms | 3.616 ± 0.078 ms | **1.647 ± 0.026 ms** |
| small/config | 2.784 ± 0.032 ms | 3.607 ± 0.117 ms | **1.608 ± 0.028 ms** |
| small/prettierrc | 2.914 ± 0.061 ms | 3.616 ± 0.028 ms | **1.684 ± 0.031 ms** |

### validate (invalid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.98 MiB | 10.02 MiB | **4.12 MiB** |
| generated/large-array | **55.78 ± 0.01 MiB** | 64.81 ± 0.01 MiB | 59.64 ± 0.01 MiB |
| generated/many-defs | 19.08 ± 0.02 MiB | 21.42 ± 0.01 MiB | **13.19 ± 0.10 MiB** |
| generated/wide-object | 15.12 MiB | 15.39 ± 0.01 MiB | **7.30 MiB** |
| large/api-catalog | 5.89 ± 0.01 MiB | 10.50 MiB | **3.95 ± 0.01 MiB** |
| large/circleci | 9.06 ± 0.01 MiB | 13.08 MiB | **5.44 ± 0.01 MiB** |
| massive/data-warehouse | 24.53 ± 0.54 MiB | 21.03 ± 0.01 MiB | **9.50 ± 0.00 MiB** |
| massive/k8s-deployment | 11.81 ± 0.02 MiB | 20.53 ± 0.03 MiB | **6.00 ± 0.01 MiB** |
| medium/package-json | 27.67 ± 0.03 MiB | — | **11.52 MiB** |
| medium/user-profile | 5.72 MiB | 10.22 ± 0.01 MiB | **3.75 MiB** |
| small/config | 5.75 ± 0.01 MiB | 10.03 ± 0.01 MiB | **3.69 MiB** |
| small/prettierrc | 5.84 MiB | 9.12 ± 0.01 MiB | **3.89 ± 0.00 MiB** |

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
