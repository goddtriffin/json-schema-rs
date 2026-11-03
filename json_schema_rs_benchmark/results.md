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
| generated/deep-nesting | **1.868 ± 0.001 ms** | 9.569 ± 0.077 ms |
| generated/large-array | **103.055 ± 0.114 µs** | 1.026 ± 0.001 ms |
| generated/many-defs | **14.193 ± 0.070 ms** | 120.406 ± 0.553 ms |
| generated/wide-object | **13.901 ± 0.016 ms** | 145.047 ± 0.307 ms |
| large/api-catalog | **379.052 ± 0.813 µs** | 3.073 ± 0.018 ms |
| large/circleci | 6.702 ± 0.055 ms | — |
| massive/data-warehouse | **29.033 ± 0.170 ms** | 268.362 ± 2.804 ms |
| massive/k8s-deployment | 15.301 ± 0.024 ms | **1.802 ± 0.001 ms** |
| medium/package-json | 43.156 ± 0.262 ms | — |
| medium/user-profile | **141.102 ± 0.211 µs** | 1.420 ± 0.007 ms |
| small/config | **52.288 ± 0.056 µs** | 508.281 ± 1.157 µs |
| small/prettierrc | 853.346 ± 14.721 µs | — |

### codegen — peak heap

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | **1000.39 KiB** | 12.65 MiB |
| generated/large-array | **70.12 KiB** | 1.23 MiB |
| generated/many-defs | **9.19 MiB** | 133.38 MiB |
| generated/wide-object | **8.66 MiB** | 162.36 MiB |
| large/api-catalog | **244.72 KiB** | 3.64 MiB |
| large/circleci | 3.88 MiB | — |
| massive/data-warehouse | **14.93 MiB** | 288.90 MiB |
| massive/k8s-deployment | 8.31 MiB | **2.79 MiB** |
| medium/package-json | 32.63 MiB | — |
| medium/user-profile | **96.99 KiB** | 1.71 MiB |
| small/config | **36.42 KiB** | 639.58 KiB |
| small/prettierrc | 527.44 KiB | — |

### schema compile — time

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 1.016 ± 0.004 ms | 206.566 ± 0.407 µs | **100.696 ± 0.154 µs** |
| generated/large-array | 44.185 ± 0.203 µs | 20.438 ± 0.051 µs | **10.541 ± 0.026 µs** |
| generated/many-defs | 13.688 ± 0.201 ms | 2.981 ± 0.007 ms | **1.632 ± 0.003 ms** |
| generated/wide-object | 23.224 ± 0.084 ms | 2.501 ± 0.002 ms | **1.301 ± 0.002 ms** |
| large/api-catalog | 176.329 ± 0.156 µs | 103.007 ± 0.126 µs | **43.492 ± 0.028 µs** |
| large/circleci | 2.858 ± 0.019 ms | 1.012 ± 0.003 ms | **511.134 ± 1.302 µs** |
| massive/data-warehouse | 45.166 ± 0.378 ms | 6.276 ± 0.017 ms | **2.626 ± 0.004 ms** |
| massive/k8s-deployment | 7.382 ± 0.082 ms | 1.585 ± 0.005 ms | **745.698 ± 0.464 µs** |
| medium/package-json | 68.053 ± 0.103 ms | 14.075 ± 0.388 ms | **5.828 ± 0.034 ms** |
| medium/user-profile | 66.539 ± 0.308 µs | 39.682 ± 0.026 µs | **16.426 ± 0.047 µs** |
| small/config | 26.775 ± 0.045 µs | 18.887 ± 0.082 µs | **7.450 ± 0.010 µs** |
| small/prettierrc | 203.700 ± 1.318 µs | 111.289 ± 1.704 µs | **58.872 ± 0.748 µs** |

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
| generated/deep-nesting | 6.287 ± 0.029 µs | **2.497 ± 0.024 µs** | 3.013 ± 0.028 µs |
| generated/large-array | 17.881 ± 0.035 ms | **7.514 ± 0.040 ms** | 8.547 ± 0.049 ms |
| generated/many-defs | 1.721 ± 0.005 ms | **628.185 ± 1.639 µs** | 745.158 ± 11.030 µs |
| generated/wide-object | 169.896 ± 0.698 µs | 109.093 ± 0.866 µs | **54.138 ± 0.048 µs** |
| large/api-catalog | 559.890 ± 0.977 ns | **236.284 ± 1.110 ns** | 420.802 ± 3.679 ns |
| large/circleci | 2.186 ± 0.013 µs | **476.708 ± 1.848 ns** | 1.001 ± 0.009 µs |
| massive/data-warehouse | 69.127 ± 0.281 µs | **24.028 ± 0.087 µs** | 44.918 ± 0.160 µs |
| massive/k8s-deployment | 515.073 ± 1.262 ns | **215.754 ± 1.285 ns** | 493.863 ± 2.898 ns |
| medium/package-json | 544.658 ± 8.220 ns | **225.942 ± 1.443 ns** | 484.615 ± 1.130 ns |
| medium/user-profile | 841.727 ± 2.807 ns | **391.706 ± 6.845 ns** | 498.106 ± 6.174 ns |
| small/config | 224.447 ± 0.410 ns | **115.058 ± 0.315 ns** | 138.907 ± 2.215 ns |
| small/prettierrc | 728.960 ± 5.795 ns | **176.709 ± 0.650 ns** | 279.409 ± 1.531 ns |

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
| generated/deep-nesting | 7.442 ± 0.030 µs | **2.895 ± 0.012 µs** | 5.122 ± 0.023 µs |
| generated/large-array | 17.866 ± 0.067 ms | **7.574 ± 0.087 ms** | 8.633 ± 0.089 ms |
| generated/many-defs | 1.734 ± 0.007 ms | **631.969 ± 0.881 µs** | 742.388 ± 3.691 µs |
| generated/wide-object | 169.958 ± 0.472 µs | 109.393 ± 0.631 µs | **54.057 ± 0.233 µs** |
| large/api-catalog | 647.305 ± 3.934 ns | **410.859 ± 2.484 ns** | 1.034 ± 0.013 µs |
| large/circleci | 127.393 ± 0.382 ns | 128.070 ± 1.961 ns | 269.799 ± 2.725 ns |
| massive/data-warehouse | 179.234 ± 1.057 µs | **137.251 ± 0.787 µs** | 268.823 ± 3.012 µs |
| massive/k8s-deployment | 178.837 ± 1.171 ns | **133.534 ± 0.503 ns** | 239.629 ± 1.133 ns |
| medium/package-json | 171.780 ± 0.257 ns | **150.450 ± 1.134 ns** | 242.142 ± 0.679 ns |
| medium/user-profile | 366.695 ± 1.372 ns | **313.504 ± 0.857 ns** | 539.676 ± 2.015 ns |
| small/config | 287.794 ± 0.873 ns | **253.489 ± 2.887 ns** | 518.373 ± 1.180 ns |
| small/prettierrc | **533.987 ± 2.970 ns** | 912.416 ± 5.521 ns | 1.545 ± 0.015 µs |

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
| generated/deep-nesting | **4.704 ± 0.515 ms †** | 109.040 ± 1.173 ms |
| generated/large-array | **2.047 ± 0.050 ms** | 87.567 ± 0.393 ms |
| generated/many-defs | **20.111 ± 0.175 ms** | 435.287 ± 2.919 ms |
| generated/wide-object | **18.927 ± 0.123 ms** | 505.234 ± 1.420 ms |
| large/api-catalog | **2.510 ± 0.037 ms** | 94.003 ± 0.189 ms |
| large/circleci | 10.739 ± 0.061 ms | — |
| massive/data-warehouse | **35.590 ± 0.622 ms** | 784.674 ± 2.754 ms |
| massive/k8s-deployment | **20.886 ± 0.155 ms** | 85.104 ± 1.256 ms |
| medium/package-json | 95.720 ± 0.337 ms | — |
| medium/user-profile | **2.080 ± 0.041 ms** | 88.285 ± 0.278 ms |
| small/config | **1.869 ± 0.037 ms** | 84.950 ± 0.358 ms |
| small/prettierrc | 3.142 ± 0.031 ms | — |

### codegen — peak RSS

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | **6.81 MiB** | 38.02 MiB |
| generated/large-array | **4.39 ± 0.01 MiB** | 33.31 ± 0.01 MiB |
| generated/many-defs | **18.97 ± 0.10 MiB** | 98.00 MiB |
| generated/wide-object | **20.31 ± 0.01 MiB** | 114.88 MiB |
| large/api-catalog | **5.14 ± 0.01 MiB** | 34.69 MiB |
| large/circleci | 10.91 ± 0.03 MiB | — |
| massive/data-warehouse | **27.39 ± 0.03 MiB** | 175.97 ± 0.08 MiB |
| massive/k8s-deployment | **17.58 MiB** | 31.77 MiB |
| medium/package-json | 38.25 MiB | — |
| medium/user-profile | **4.59 ± 0.00 MiB** | 33.58 MiB |
| small/config | **4.23 ± 0.02 MiB** | 32.92 ± 0.02 MiB |
| small/prettierrc | 5.62 ± 0.02 MiB | — |

### validate (valid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.451 ± 0.420 ms | 4.052 ± 0.051 ms | **1.796 ± 0.047 ms** |
| generated/large-array | 58.251 ± 0.271 ms | 44.662 ± 0.429 ms | 43.694 ± 0.510 ms |
| generated/many-defs | 22.844 ± 0.268 ms | 11.444 ± 0.097 ms | **7.348 ± 0.076 ms** |
| generated/wide-object | 27.728 ± 0.151 ms | 8.060 ± 0.103 ms | **3.806 ± 0.034 ms** |
| large/api-catalog | 3.097 ± 0.044 ms | 3.857 ± 0.067 ms | **1.719 ± 0.032 ms** |
| large/circleci | 6.793 ± 0.080 ms | 5.664 ± 0.060 ms | **2.521 ± 0.024 ms** |
| massive/data-warehouse | 49.666 ± 0.346 ms | 11.902 ± 0.153 ms | **5.325 ± 0.042 ms** |
| massive/k8s-deployment | 11.658 ± 0.072 ms | 503.008 ± 113.046 ms † | **2.889 ± 0.042 ms** |
| medium/package-json | 74.255 ± 0.126 ms | — | **7.514 ± 0.127 ms** |
| medium/user-profile | 2.875 ± 0.045 ms | 3.672 ± 0.168 ms | **1.633 ± 0.028 ms** |
| small/config | 2.818 ± 0.037 ms | 3.652 ± 0.098 ms | **1.603 ± 0.029 ms** |
| small/prettierrc | 3.047 ± 0.030 ms | 3.624 ± 0.038 ms | **1.697 ± 0.039 ms** |

### validate (valid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.97 MiB | 9.86 ± 0.01 MiB | **4.14 ± 0.01 MiB** |
| generated/large-array | **55.77 ± 0.01 MiB** | 64.69 ± 0.01 MiB | 59.62 ± 0.02 MiB |
| generated/many-defs | 19.08 MiB | 21.34 ± 0.04 MiB | **13.14 MiB** |
| generated/wide-object | 15.08 ± 0.01 MiB | 15.30 ± 0.01 MiB | **7.34 MiB** |
| large/api-catalog | 5.89 ± 0.01 MiB | 10.36 MiB | **3.89 ± 0.01 MiB** |
| large/circleci | 9.06 ± 0.01 MiB | 13.09 ± 0.02 MiB | **5.42 ± 0.00 MiB** |
| massive/data-warehouse | 22.05 MiB | 21.00 MiB | **9.42 ± 0.01 MiB** |
| massive/k8s-deployment | 11.80 MiB | 20.61 ± 0.05 MiB | **6.05 ± 0.02 MiB** |
| medium/package-json | 27.67 MiB | — | **11.61 MiB** |
| medium/user-profile | 5.73 ± 0.00 MiB | 10.05 ± 0.01 MiB | **3.78 MiB** |
| small/config | 5.72 ± 0.01 MiB | 9.86 ± 0.01 MiB | **3.66 ± 0.01 MiB** |
| small/prettierrc | 5.80 ± 0.02 MiB | 8.95 MiB | **3.83 MiB** |

### validate (invalid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.163 ± 0.060 ms | 4.067 ± 0.115 ms | **1.790 ± 0.041 ms** |
| generated/large-array | 58.601 ± 1.009 ms | 44.940 ± 0.441 ms | 43.690 ± 0.328 ms |
| generated/many-defs | 22.987 ± 0.248 ms | 11.382 ± 0.132 ms | **7.345 ± 0.065 ms** |
| generated/wide-object | 27.723 ± 0.172 ms | 7.956 ± 0.068 ms | **3.803 ± 0.029 ms** |
| large/api-catalog | 3.107 ± 0.025 ms | 3.882 ± 0.061 ms | **1.722 ± 0.048 ms** |
| large/circleci | 6.766 ± 0.053 ms | 5.622 ± 0.058 ms | **2.504 ± 0.020 ms** |
| massive/data-warehouse | 50.669 ± 0.229 ms | 11.994 ± 0.122 ms | **5.485 ± 0.034 ms** |
| massive/k8s-deployment | 11.689 ± 0.094 ms | 487.376 ± 90.257 ms † | **2.891 ± 0.031 ms** |
| medium/package-json | 74.149 ± 0.108 ms | — | **7.509 ± 0.070 ms** |
| medium/user-profile | 2.885 ± 0.137 ms | 3.645 ± 0.065 ms | **1.632 ± 0.033 ms** |
| small/config | 2.829 ± 0.029 ms | 3.580 ± 0.071 ms | **1.600 ± 0.025 ms** |
| small/prettierrc | 3.032 ± 0.026 ms | 3.672 ± 0.047 ms | **1.698 ± 0.030 ms** |

### validate (invalid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.98 MiB | 10.03 ± 0.01 MiB | **4.23 ± 0.02 MiB** |
| generated/large-array | **55.81 ± 0.01 MiB** | 64.81 ± 0.01 MiB | 59.69 ± 0.01 MiB |
| generated/many-defs | 19.09 ± 0.01 MiB | 21.44 MiB | **13.19 MiB** |
| generated/wide-object | 15.12 MiB | 15.42 ± 0.01 MiB | **7.38 ± 0.02 MiB** |
| large/api-catalog | 5.91 ± 0.01 MiB | 10.50 MiB | **3.98 ± 0.01 MiB** |
| large/circleci | 9.06 ± 0.02 MiB | 13.12 ± 0.03 MiB | **5.52 ± 0.01 MiB** |
| massive/data-warehouse | 22.75 ± 0.01 MiB | 21.03 ± 0.01 MiB | **9.66 MiB** |
| massive/k8s-deployment | 11.81 ± 0.02 MiB | 20.53 MiB | **6.12 ± 0.03 MiB** |
| medium/package-json | 27.66 ± 0.02 MiB | — | **11.62 ± 0.01 MiB** |
| medium/user-profile | 5.75 ± 0.03 MiB | 10.22 ± 0.01 MiB | **3.83 MiB** |
| small/config | 5.75 ± 0.01 MiB | 10.02 ± 0.01 MiB | **3.80 MiB** |
| small/prettierrc | 5.84 ± 0.00 MiB | 9.14 MiB | **3.92 ± 0.02 MiB** |

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
