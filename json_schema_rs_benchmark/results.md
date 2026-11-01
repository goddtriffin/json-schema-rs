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
| generated/deep-nesting | **371.682 ± 0.919 µs** | 9.576 ± 0.020 ms |
| generated/large-array | **44.546 ± 0.029 µs** | 1.025 ± 0.034 ms |
| generated/many-defs | **4.401 ± 0.023 ms** | 121.145 ± 2.340 ms |
| generated/wide-object | **5.576 ± 0.017 ms** | 147.705 ± 1.112 ms |
| large/api-catalog | **163.053 ± 0.335 µs** | 3.077 ± 0.009 ms |
| large/circleci | 1.654 ± 0.005 ms | — |
| massive/data-warehouse | **8.450 ± 0.047 ms** | 268.759 ± 3.932 ms |
| massive/k8s-deployment | 4.893 ± 0.297 ms | **1.853 ± 0.014 ms** |
| medium/package-json | 16.126 ± 0.654 ms | — |
| medium/user-profile | **60.869 ± 0.240 µs** | 1.420 ± 0.039 ms |
| small/config | **20.812 ± 0.169 µs** | 513.101 ± 3.025 µs |
| small/prettierrc | 324.572 ± 1.649 µs | — |

### codegen — peak heap

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | **343.19 KiB** | 12.65 MiB |
| generated/large-array | **31.29 KiB** | 1.23 MiB |
| generated/many-defs | **3.28 MiB** | 133.38 MiB |
| generated/wide-object | **4.66 MiB** | 162.36 MiB |
| large/api-catalog | **96.66 KiB** | 3.64 MiB |
| large/circleci | 1.55 MiB | — |
| massive/data-warehouse | **5.65 MiB** | 288.90 MiB |
| massive/k8s-deployment | 3.42 MiB | **2.79 MiB** |
| medium/package-json | 15.63 MiB | — |
| medium/user-profile | **41.86 KiB** | 1.71 MiB |
| small/config | **16.29 KiB** | 639.58 KiB |
| small/prettierrc | 190.39 KiB | — |

### schema compile — time

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 1.032 ± 0.006 ms | 205.972 ± 1.796 µs | **100.062 ± 0.235 µs** |
| generated/large-array | 44.847 ± 0.085 µs | 20.786 ± 0.049 µs | **10.781 ± 0.170 µs** |
| generated/many-defs | 13.835 ± 0.092 ms | 2.987 ± 0.040 ms | **1.616 ± 0.005 ms** |
| generated/wide-object | 23.929 ± 0.753 ms | 2.528 ± 0.021 ms | **1.304 ± 0.021 ms** |
| large/api-catalog | 178.309 ± 1.686 µs | 103.287 ± 1.079 µs | **44.221 ± 0.316 µs** |
| large/circleci | 2.858 ± 0.037 ms | 1.014 ± 0.011 ms | **509.962 ± 2.481 µs** |
| massive/data-warehouse | 45.440 ± 0.157 ms | 6.300 ± 0.058 ms | **2.620 ± 0.008 ms** |
| massive/k8s-deployment | 7.344 ± 0.026 ms | 1.583 ± 0.060 ms | **742.674 ± 2.251 µs** |
| medium/package-json | 68.631 ± 0.253 ms | 14.011 ± 0.121 ms | **5.777 ± 0.039 ms** |
| medium/user-profile | 66.465 ± 0.596 µs | 39.356 ± 0.126 µs | **16.319 ± 0.031 µs** |
| small/config | 26.797 ± 0.056 µs | 18.839 ± 0.082 µs | **7.418 ± 0.022 µs** |
| small/prettierrc | 202.116 ± 0.345 µs | 110.948 ± 0.953 µs | **56.238 ± 0.134 µs** |

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
| generated/deep-nesting | 6.456 ± 0.020 µs | **2.435 ± 0.006 µs** | 2.992 ± 0.038 µs |
| generated/large-array | 17.870 ± 0.411 ms | **7.268 ± 0.041 ms** | 8.653 ± 0.329 ms |
| generated/many-defs | 1.733 ± 0.002 ms | **613.353 ± 3.476 µs** | 753.700 ± 13.269 µs |
| generated/wide-object | 170.081 ± 0.628 µs | 112.193 ± 1.787 µs | **54.020 ± 0.627 µs** |
| large/api-catalog | 569.684 ± 3.937 ns | **240.120 ± 3.840 ns** | 424.360 ± 7.154 ns |
| large/circleci | 2.266 ± 0.044 µs | **482.187 ± 4.063 ns** | 1.031 ± 0.018 µs |
| massive/data-warehouse | 70.197 ± 0.408 µs | **24.378 ± 0.168 µs** | 45.565 ± 0.540 µs |
| massive/k8s-deployment | 527.202 ± 3.494 ns | **214.022 ± 1.021 ns** | 501.429 ± 2.689 ns |
| medium/package-json | 548.530 ± 10.305 ns | **239.461 ± 3.523 ns** | 507.259 ± 3.158 ns |
| medium/user-profile | 834.973 ± 6.745 ns | **377.616 ± 5.239 ns** | 505.930 ± 10.761 ns |
| small/config | 224.078 ± 0.783 ns | **112.297 ± 0.515 ns** | 145.590 ± 2.742 ns |
| small/prettierrc | 719.378 ± 2.274 ns | **172.964 ± 1.360 ns** | 279.430 ± 3.951 ns |

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
| generated/deep-nesting | 7.587 ± 0.085 µs | **2.797 ± 0.015 µs** | 5.146 ± 0.062 µs |
| generated/large-array | 18.152 ± 0.274 ms | **7.776 ± 0.487 ms** | 8.863 ± 0.435 ms |
| generated/many-defs | 1.973 ± 0.031 ms | **686.984 ± 38.041 µs** | 782.382 ± 14.518 µs |
| generated/wide-object | 184.310 ± 3.246 µs | 125.934 ± 2.960 µs | **59.724 ± 1.126 µs** |
| large/api-catalog | 716.965 ± 8.617 ns | **419.567 ± 8.023 ns** | 1.118 ± 0.026 µs |
| large/circleci | **134.970 ± 1.902 ns** | 141.387 ± 2.437 ns | 304.375 ± 28.182 ns |
| massive/data-warehouse | 191.176 ± 5.875 µs | **153.789 ± 4.467 µs** | 293.595 ± 5.789 µs |
| massive/k8s-deployment | 187.576 ± 4.144 ns | **138.922 ± 5.037 ns** | 262.678 ± 4.142 ns |
| medium/package-json | 169.528 ± 0.573 ns | **152.208 ± 2.585 ns** | 253.363 ± 4.235 ns |
| medium/user-profile | 375.001 ± 23.124 ns | **322.009 ± 8.177 ns** | 545.209 ± 5.183 ns |
| small/config | 285.566 ± 3.492 ns | **245.475 ± 1.956 ns** | 546.982 ± 16.136 ns |
| small/prettierrc | **522.654 ± 0.403 ns** | 967.665 ± 21.483 ns | 1.565 ± 0.021 µs |

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
| generated/deep-nesting | **2.824 ± 0.326 ms †** | 109.117 ± 1.747 ms |
| generated/large-array | **1.847 ± 0.031 ms** | 88.175 ± 1.067 ms |
| generated/many-defs | **7.891 ± 0.229 ms** | 442.264 ± 4.127 ms |
| generated/wide-object | **9.093 ± 0.116 ms** | 513.124 ± 4.343 ms |
| large/api-catalog | **2.232 ± 0.055 ms** | 98.771 ± 2.246 ms |
| large/circleci | 4.553 ± 0.084 ms | — |
| massive/data-warehouse | **12.612 ± 0.154 ms** | 823.812 ± 20.637 ms |
| massive/k8s-deployment | **9.009 ± 0.174 ms** | 93.820 ± 1.019 ms |
| medium/package-json | 37.844 ± 1.053 ms | — |
| medium/user-profile | **2.442 ± 1.632 ms †** | 96.650 ± 1.265 ms |
| small/config | **2.253 ± 0.223 ms** | 94.868 ± 1.431 ms |
| small/prettierrc | 2.695 ± 0.103 ms | — |

### codegen — peak RSS

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | **5.00 ± 0.01 MiB** | 37.98 ± 0.01 MiB |
| generated/large-array | **3.83 MiB** | 33.28 ± 0.01 MiB |
| generated/many-defs | **8.78 ± 0.01 MiB** | 98.19 MiB |
| generated/wide-object | **11.25 ± 0.03 MiB** | 115.27 ± 0.08 MiB |
| large/api-catalog | **4.28 MiB** | 34.66 ± 0.02 MiB |
| large/circleci | 6.67 ± 0.03 MiB | — |
| massive/data-warehouse | **11.62 ± 0.00 MiB** | 175.94 ± 0.43 MiB |
| massive/k8s-deployment | **9.44 ± 0.14 MiB** | 31.78 ± 0.02 MiB |
| medium/package-json | 20.36 ± 0.23 MiB | — |
| medium/user-profile | **3.89 ± 0.02 MiB** | 33.61 ± 0.06 MiB |
| small/config | **3.67 MiB** | 32.94 MiB |
| small/prettierrc | 4.70 ± 0.03 MiB | — |

### validate (valid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.551 ± 0.501 ms † | 4.221 ± 0.190 ms | **2.108 ± 0.081 ms** |
| generated/large-array | 58.610 ± 1.158 ms | 44.709 ± 0.539 ms | 44.272 ± 0.467 ms |
| generated/many-defs | 23.156 ± 0.557 ms | 11.447 ± 0.282 ms | **7.097 ± 0.080 ms** |
| generated/wide-object | 28.135 ± 0.857 ms | 8.097 ± 0.105 ms | **4.105 ± 0.136 ms** |
| large/api-catalog | 3.292 ± 0.065 ms | 4.142 ± 0.096 ms | **1.900 ± 0.049 ms** |
| large/circleci | 6.907 ± 0.135 ms | 5.880 ± 0.189 ms | **2.676 ± 0.023 ms** |
| massive/data-warehouse | 52.661 ± 1.356 ms | 12.769 ± 0.656 ms | **6.025 ± 0.100 ms** |
| massive/k8s-deployment | 12.761 ± 0.199 ms | 253.553 ± 40.074 ms † | **3.388 ± 0.154 ms** |
| medium/package-json | 78.684 ± 1.072 ms | — | — |
| medium/user-profile | 3.472 ± 0.196 ms | 4.472 ± 0.196 ms | **1.992 ± 0.082 ms** |
| small/config | 3.480 ± 0.241 ms | 4.443 ± 0.461 ms † | **1.986 ± 0.184 ms** |
| small/prettierrc | 3.582 ± 0.108 ms | 4.492 ± 0.479 ms † | **2.129 ± 0.192 ms** |

### validate (valid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.97 MiB | 9.89 ± 0.01 MiB | **4.02 MiB** |
| generated/large-array | **55.78 MiB** | 64.67 ± 0.01 MiB | 59.50 MiB |
| generated/many-defs | 19.08 ± 0.02 MiB | 21.33 ± 0.02 MiB | **12.89 ± 0.05 MiB** |
| generated/wide-object | 16.45 ± 0.35 MiB | 15.27 ± 0.01 MiB | **7.23 ± 0.00 MiB** |
| large/api-catalog | 5.88 ± 0.01 MiB | 10.36 ± 0.01 MiB | **3.73 MiB** |
| large/circleci | 9.08 MiB | 13.12 MiB | **5.36 ± 0.01 MiB** |
| massive/data-warehouse | 23.19 ± 0.04 MiB | 21.11 ± 0.05 MiB | **9.42 ± 0.01 MiB** |
| massive/k8s-deployment | 11.80 MiB | 20.62 MiB | **6.00 ± 0.02 MiB** |
| medium/package-json | 28.77 ± 0.15 MiB | — | — |
| medium/user-profile | 5.78 ± 0.03 MiB | 10.09 MiB | **3.64 ± 0.01 MiB** |
| small/config | 5.72 ± 0.01 MiB | 9.84 ± 0.01 MiB | **3.58 MiB** |
| small/prettierrc | 5.84 ± 0.02 MiB | 8.98 ± 0.02 MiB | **3.72 MiB** |

### validate (invalid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.280 ± 0.130 ms | 4.288 ± 0.092 ms | **2.040 ± 0.051 ms** |
| generated/large-array | 58.243 ± 0.258 ms | 44.625 ± 1.446 ms | 44.127 ± 0.396 ms |
| generated/many-defs | 23.140 ± 1.250 ms | 11.412 ± 0.075 ms | **7.016 ± 0.100 ms** |
| generated/wide-object | 28.424 ± 0.506 ms | 8.736 ± 0.141 ms | **3.947 ± 0.111 ms** |
| large/api-catalog | 3.302 ± 0.161 ms | 4.061 ± 0.056 ms | **1.866 ± 0.017 ms** |
| large/circleci | 7.026 ± 0.158 ms | 5.767 ± 0.078 ms | **2.605 ± 0.060 ms** |
| massive/data-warehouse | 53.672 ± 1.114 ms | 13.306 ± 0.723 ms | **6.122 ± 0.195 ms** |
| massive/k8s-deployment | 12.822 ± 0.208 ms | 264.973 ± 36.593 ms † | **3.288 ± 0.182 ms** |
| medium/package-json | 78.517 ± 0.907 ms | — | — |
| medium/user-profile | 3.479 ± 0.145 ms | 4.356 ± 0.169 ms | **1.899 ± 0.076 ms** |
| small/config | 3.425 ± 0.129 ms | 4.303 ± 0.424 ms | **1.933 ± 0.096 ms** |
| small/prettierrc | 3.651 ± 0.199 ms | 4.346 ± 0.164 ms | **2.050 ± 0.112 ms** |

### validate (invalid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.98 ± 0.02 MiB | 10.02 MiB | **4.06 ± 0.01 MiB** |
| generated/large-array | **55.81 MiB** | 64.81 MiB | 59.52 ± 0.01 MiB |
| generated/many-defs | 20.19 ± 0.46 MiB | 21.39 ± 0.02 MiB | **12.89 ± 0.02 MiB** |
| generated/wide-object | 15.12 MiB | 15.42 ± 0.01 MiB | **7.25 MiB** |
| large/api-catalog | 5.89 ± 0.01 MiB | 10.48 ± 0.00 MiB | **3.75 ± 0.01 MiB** |
| large/circleci | 9.34 ± 0.12 MiB | 13.05 ± 0.01 MiB | **5.38 ± 0.01 MiB** |
| massive/data-warehouse | 23.66 ± 0.33 MiB | 21.77 ± 0.38 MiB | **9.52 MiB** |
| massive/k8s-deployment | 11.80 ± 0.01 MiB | 20.58 ± 0.03 MiB | **6.00 MiB** |
| medium/package-json | 28.77 ± 0.30 MiB | — | — |
| medium/user-profile | 5.75 ± 0.01 MiB | 10.19 ± 0.02 MiB | **3.64 MiB** |
| small/config | 5.75 ± 0.01 MiB | 10.00 ± 0.01 MiB | **3.59 MiB** |
| small/prettierrc | 5.86 ± 0.11 MiB | 9.16 ± 0.01 MiB | **3.72 ± 0.00 MiB** |

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
