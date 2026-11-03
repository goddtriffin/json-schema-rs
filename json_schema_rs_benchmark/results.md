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
| generated/deep-nesting | **3.042 ± 0.006 ms** | 9.576 ± 0.046 ms |
| generated/large-array | **134.228 ± 0.293 µs** | 1.032 ± 0.001 ms |
| generated/many-defs | **20.298 ± 0.048 ms** | 121.292 ± 0.734 ms |
| generated/wide-object | **17.865 ± 0.020 ms** | 147.189 ± 2.601 ms |
| large/api-catalog | **461.104 ± 0.241 µs** | 3.093 ± 0.003 ms |
| large/circleci | 6.862 ± 0.006 ms | — |
| massive/data-warehouse | **36.511 ± 0.171 ms** | 268.557 ± 0.883 ms |
| massive/k8s-deployment | 15.636 ± 0.061 ms | **1.800 ± 0.004 ms** |
| medium/package-json | 43.786 ± 0.117 ms | — |
| medium/user-profile | **185.705 ± 0.513 µs** | 1.422 ± 0.017 ms |
| small/config | **64.228 ± 0.249 µs** | 512.942 ± 0.620 µs |
| small/prettierrc | 866.480 ± 2.903 µs | — |

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
| medium/package-json | 32.87 MiB | — |
| medium/user-profile | **116.96 KiB** | 1.71 MiB |
| small/config | **40.64 KiB** | 639.58 KiB |
| small/prettierrc | 528.44 KiB | — |

### schema compile — time

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 1.021 ± 0.004 ms | 205.814 ± 0.643 µs | **101.002 ± 0.241 µs** |
| generated/large-array | 44.510 ± 0.050 µs | 20.551 ± 0.088 µs | **10.718 ± 0.096 µs** |
| generated/many-defs | 13.699 ± 0.010 ms | 2.983 ± 0.004 ms | **1.626 ± 0.003 ms** |
| generated/wide-object | 23.158 ± 0.043 ms | 2.510 ± 0.018 ms | **1.309 ± 0.005 ms** |
| large/api-catalog | 176.640 ± 1.193 µs | 103.281 ± 0.387 µs | **43.391 ± 0.052 µs** |
| large/circleci | 2.843 ± 0.001 ms | 1.012 ± 0.010 ms | **509.477 ± 0.457 µs** |
| massive/data-warehouse | 44.650 ± 0.140 ms | 6.252 ± 0.009 ms | **2.621 ± 0.003 ms** |
| massive/k8s-deployment | 7.323 ± 0.048 ms | 1.596 ± 0.006 ms | **742.545 ± 0.400 µs** |
| medium/package-json | 68.185 ± 0.264 ms | 13.952 ± 0.032 ms | **5.840 ± 0.031 ms** |
| medium/user-profile | 66.248 ± 0.118 µs | 39.534 ± 0.046 µs | **16.447 ± 0.101 µs** |
| small/config | 26.767 ± 0.042 µs | 18.989 ± 0.051 µs | **7.447 ± 0.019 µs** |
| small/prettierrc | 202.512 ± 1.161 µs | 111.921 ± 0.853 µs | **56.664 ± 0.073 µs** |

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
| generated/deep-nesting | 6.338 ± 0.013 µs | **2.514 ± 0.037 µs** | 2.941 ± 0.069 µs |
| generated/large-array | 17.849 ± 0.122 ms | **7.461 ± 0.072 ms** | 8.512 ± 0.073 ms |
| generated/many-defs | 1.749 ± 0.005 ms | **629.169 ± 1.666 µs** | 744.813 ± 5.593 µs |
| generated/wide-object | 169.524 ± 0.517 µs | 114.715 ± 0.588 µs | **54.558 ± 0.251 µs** |
| large/api-catalog | 562.603 ± 4.991 ns | **238.072 ± 1.517 ns** | 415.769 ± 2.943 ns |
| large/circleci | 2.225 ± 0.014 µs | **471.701 ± 1.286 ns** | 999.104 ± 9.241 ns |
| massive/data-warehouse | 69.641 ± 0.450 µs | **24.042 ± 0.077 µs** | 45.106 ± 0.412 µs |
| massive/k8s-deployment | 515.184 ± 0.712 ns | **216.662 ± 0.593 ns** | 493.556 ± 1.202 ns |
| medium/package-json | 533.907 ± 0.901 ns | **230.973 ± 1.276 ns** | 484.429 ± 1.082 ns |
| medium/user-profile | 837.939 ± 2.551 ns | **378.596 ± 7.144 ns** | 489.735 ± 4.839 ns |
| small/config | 222.046 ± 0.367 ns | **113.491 ± 0.333 ns** | 145.170 ± 3.055 ns |
| small/prettierrc | 723.141 ± 3.481 ns | **174.727 ± 0.564 ns** | 276.726 ± 0.868 ns |

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
| generated/deep-nesting | 7.389 ± 0.032 µs | **2.902 ± 0.026 µs** | 5.082 ± 0.036 µs |
| generated/large-array | 17.774 ± 0.053 ms | **7.472 ± 0.048 ms** | 8.631 ± 0.084 ms |
| generated/many-defs | 1.753 ± 0.006 ms | **624.485 ± 1.496 µs** | 748.126 ± 3.155 µs |
| generated/wide-object | 170.327 ± 0.566 µs | 108.739 ± 4.758 µs | **54.004 ± 0.639 µs** |
| large/api-catalog | 655.014 ± 0.903 ns | **397.396 ± 0.987 ns** | 1.025 ± 0.010 µs |
| large/circleci | 126.758 ± 0.072 ns | 129.612 ± 2.390 ns | 273.023 ± 1.280 ns |
| massive/data-warehouse | 178.900 ± 1.330 µs | **139.592 ± 0.862 µs** | 271.852 ± 2.023 µs |
| massive/k8s-deployment | 175.151 ± 0.562 ns | **135.410 ± 1.019 ns** | 245.519 ± 1.027 ns |
| medium/package-json | 171.206 ± 0.581 ns | **151.319 ± 0.593 ns** | 244.574 ± 0.370 ns |
| medium/user-profile | 362.499 ± 0.541 ns | **318.295 ± 1.778 ns** | 547.093 ± 3.412 ns |
| small/config | 286.943 ± 0.501 ns | **247.037 ± 0.521 ns** | 534.864 ± 3.451 ns |
| small/prettierrc | **532.385 ± 1.265 ns** | 911.795 ± 4.994 ns | 1.511 ± 0.004 µs |

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
| generated/deep-nesting | **5.971 ± 0.486 ms** | 109.053 ± 0.675 ms |
| generated/large-array | **2.099 ± 0.075 ms** | 87.572 ± 0.414 ms |
| generated/many-defs | **28.050 ± 0.170 ms** | 437.085 ± 1.117 ms |
| generated/wide-object | **24.798 ± 0.231 ms** | 505.263 ± 1.962 ms |
| large/api-catalog | **2.653 ± 0.040 ms** | 93.677 ± 1.075 ms |
| large/circleci | 10.926 ± 0.068 ms | — |
| massive/data-warehouse | **43.425 ± 0.351 ms** | 784.399 ± 2.457 ms |
| massive/k8s-deployment | **21.332 ± 0.166 ms** | 85.393 ± 0.796 ms |
| medium/package-json | 96.988 ± 0.291 ms | — |
| medium/user-profile | **2.227 ± 0.064 ms** | 87.518 ± 1.383 ms |
| small/config | **1.939 ± 0.065 ms** | 84.627 ± 0.486 ms |
| small/prettierrc | 3.202 ± 0.055 ms | — |

### codegen — peak RSS

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | **7.61 MiB** | 38.00 ± 0.01 MiB |
| generated/large-array | **4.62 MiB** | 33.28 ± 0.02 MiB |
| generated/many-defs | **22.23 ± 0.08 MiB** | 98.00 ± 0.01 MiB |
| generated/wide-object | **22.66 ± 0.05 MiB** | 114.62 ± 0.41 MiB |
| large/api-catalog | **5.31 MiB** | 34.66 ± 0.01 MiB |
| large/circleci | 11.08 ± 0.01 MiB | — |
| massive/data-warehouse | **31.48 ± 0.05 MiB** | 175.95 ± 0.02 MiB |
| massive/k8s-deployment | **17.56 ± 0.01 MiB** | 31.80 ± 0.01 MiB |
| medium/package-json | 39.56 ± 0.01 MiB | — |
| medium/user-profile | **4.89 ± 0.02 MiB** | 33.56 MiB |
| small/config | **4.30 ± 0.01 MiB** | 32.95 ± 0.04 MiB |
| small/prettierrc | 5.64 ± 0.01 MiB | — |

### validate (valid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.484 ± 0.437 ms | 3.978 ± 0.036 ms | **1.779 ± 0.035 ms** |
| generated/large-array | 58.429 ± 0.182 ms | 44.818 ± 1.002 ms | 44.146 ± 0.333 ms |
| generated/many-defs | 23.336 ± 0.488 ms | 11.274 ± 0.126 ms | **7.323 ± 0.082 ms** |
| generated/wide-object | 27.818 ± 0.206 ms | 7.978 ± 0.099 ms | **3.797 ± 0.051 ms** |
| large/api-catalog | 3.083 ± 0.039 ms | 3.860 ± 0.136 ms | **1.710 ± 0.041 ms** |
| large/circleci | 6.796 ± 0.072 ms | 5.751 ± 0.071 ms | **2.508 ± 0.036 ms** |
| massive/data-warehouse | 49.555 ± 0.227 ms | 11.862 ± 0.091 ms | **5.288 ± 0.054 ms** |
| massive/k8s-deployment | 11.659 ± 0.080 ms | 311.529 ± 58.448 ms † | **2.831 ± 0.034 ms** |
| medium/package-json | 74.320 ± 2.015 ms | — | **7.506 ± 0.054 ms** |
| medium/user-profile | 2.890 ± 0.052 ms | 3.648 ± 0.045 ms | **1.653 ± 0.035 ms** |
| small/config | 2.822 ± 0.034 ms | 3.606 ± 0.034 ms | **1.608 ± 0.036 ms** |
| small/prettierrc | 3.047 ± 0.038 ms | 3.682 ± 0.054 ms | **1.717 ± 0.046 ms** |

### validate (valid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.97 MiB | 9.88 MiB | **4.12 ± 0.01 MiB** |
| generated/large-array | **55.78 ± 0.01 MiB** | 64.67 ± 0.01 MiB | 59.64 ± 0.01 MiB |
| generated/many-defs | 19.08 ± 0.02 MiB | 21.36 MiB | **13.22 MiB** |
| generated/wide-object | 15.08 MiB | 15.30 MiB | **7.34 MiB** |
| large/api-catalog | 5.89 MiB | 10.36 MiB | **3.91 ± 0.00 MiB** |
| large/circleci | 9.06 ± 0.03 MiB | 13.09 ± 0.02 MiB | **5.45 ± 0.01 MiB** |
| massive/data-warehouse | 22.02 ± 0.02 MiB | 21.00 MiB | **9.52 ± 0.01 MiB** |
| massive/k8s-deployment | 11.80 ± 0.01 MiB | 20.50 ± 0.08 MiB | **5.97 ± 0.01 MiB** |
| medium/package-json | 28.44 ± 0.39 MiB | — | **11.62 ± 0.02 MiB** |
| medium/user-profile | 5.72 MiB | 10.08 MiB | **3.78 MiB** |
| small/config | 5.72 ± 0.00 MiB | 9.84 ± 0.00 MiB | **3.66 MiB** |
| small/prettierrc | 5.84 ± 0.02 MiB | 8.95 MiB | **3.89 ± 0.01 MiB** |

### validate (invalid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.063 ± 0.049 ms | 4.012 ± 0.036 ms | **1.798 ± 0.024 ms** |
| generated/large-array | 58.530 ± 0.386 ms | 44.773 ± 0.464 ms | 43.986 ± 0.501 ms |
| generated/many-defs | 22.892 ± 0.218 ms | 11.289 ± 0.090 ms | **7.301 ± 0.057 ms** |
| generated/wide-object | 27.685 ± 0.206 ms | 8.086 ± 0.129 ms | **3.807 ± 0.025 ms** |
| large/api-catalog | 3.097 ± 0.027 ms | 3.830 ± 0.047 ms | **1.731 ± 0.032 ms** |
| large/circleci | 6.760 ± 0.039 ms | 5.558 ± 0.058 ms | **2.530 ± 0.044 ms** |
| massive/data-warehouse | 50.595 ± 0.247 ms | 11.872 ± 0.115 ms | **5.439 ± 0.054 ms** |
| massive/k8s-deployment | 11.785 ± 0.127 ms | 301.273 ± 52.653 ms † | **2.837 ± 0.029 ms** |
| medium/package-json | 74.069 ± 0.380 ms | — | **7.456 ± 0.077 ms** |
| medium/user-profile | 2.897 ± 0.037 ms | 3.731 ± 0.114 ms | **1.655 ± 0.020 ms** |
| small/config | 2.788 ± 0.039 ms | 3.619 ± 0.053 ms | **1.601 ± 0.030 ms** |
| small/prettierrc | 3.061 ± 0.045 ms | 3.701 ± 0.091 ms | **1.722 ± 0.313 ms †** |

### validate (invalid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.98 MiB | 10.00 ± 0.04 MiB | **4.20 ± 0.01 MiB** |
| generated/large-array | **55.83 ± 0.01 MiB** | 64.81 MiB | 59.70 MiB |
| generated/many-defs | 19.11 MiB | 21.42 ± 0.01 MiB | **13.23 ± 0.01 MiB** |
| generated/wide-object | 15.08 ± 0.02 MiB | 15.41 ± 0.01 MiB | **7.41 ± 0.01 MiB** |
| large/api-catalog | 5.89 ± 0.01 MiB | 10.50 ± 0.00 MiB | **3.97 ± 0.01 MiB** |
| large/circleci | 9.06 MiB | 13.11 MiB | **5.52 MiB** |
| massive/data-warehouse | 24.22 MiB | 21.06 ± 0.01 MiB | **9.77 MiB** |
| massive/k8s-deployment | 11.81 MiB | 20.58 ± 0.04 MiB | **6.02 ± 0.01 MiB** |
| medium/package-json | 27.61 ± 0.04 MiB | — | **11.67 ± 0.01 MiB** |
| medium/user-profile | 5.72 ± 0.02 MiB | 10.22 MiB | **3.81 ± 0.01 MiB** |
| small/config | 5.75 ± 0.02 MiB | 10.02 MiB | **3.75 ± 0.00 MiB** |
| small/prettierrc | 5.86 ± 0.01 MiB | 9.11 ± 0.01 MiB | **3.94 ± 0.01 MiB** |

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
