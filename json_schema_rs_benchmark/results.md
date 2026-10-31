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
| generated/deep-nesting | **374.877 ± 11.931 µs** | 9.994 ± 0.432 ms |
| generated/large-array | **45.050 ± 0.160 µs** | 1.037 ± 0.020 ms |
| generated/many-defs | **4.351 ± 0.028 ms** | 121.525 ± 2.297 ms |
| generated/wide-object | **5.630 ± 0.206 ms** | 147.847 ± 2.543 ms |
| large/api-catalog | **165.890 ± 2.362 µs** | 3.189 ± 0.034 ms |
| large/circleci | 1.707 ± 0.023 ms | — |
| massive/data-warehouse | **8.497 ± 0.124 ms** | 271.727 ± 3.535 ms |
| massive/k8s-deployment | 4.738 ± 0.080 ms | **1.815 ± 0.023 ms** |
| medium/package-json | 16.249 ± 0.227 ms | — |
| medium/user-profile | **60.594 ± 0.214 µs** | 1.422 ± 0.028 ms |
| small/config | **20.601 ± 0.228 µs** | 521.567 ± 7.488 µs |
| small/prettierrc | 328.509 ± 4.535 µs | — |

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
| generated/deep-nesting | 1.038 ± 0.010 ms | 208.561 ± 2.781 µs | **100.946 ± 0.397 µs** |
| generated/large-array | 44.950 ± 0.503 µs | 20.922 ± 0.618 µs | **10.622 ± 0.138 µs** |
| generated/many-defs | 13.766 ± 0.076 ms | 2.999 ± 0.032 ms | **1.625 ± 0.005 ms** |
| generated/wide-object | 23.056 ± 0.178 ms | 2.520 ± 0.029 ms | **1.306 ± 0.013 ms** |
| large/api-catalog | 179.735 ± 2.811 µs | 104.228 ± 0.521 µs | **43.396 ± 0.392 µs** |
| large/circleci | 2.863 ± 0.017 ms | 1.017 ± 0.015 ms | **511.053 ± 2.611 µs** |
| massive/data-warehouse | 44.020 ± 0.417 ms | 6.417 ± 0.085 ms | **2.643 ± 0.030 ms** |
| massive/k8s-deployment | 7.340 ± 0.131 ms | 1.584 ± 0.005 ms | **741.742 ± 7.311 µs** |
| medium/package-json | 70.173 ± 1.133 ms | 14.343 ± 0.998 ms | **5.861 ± 0.090 ms** |
| medium/user-profile | 67.580 ± 0.493 µs | 40.108 ± 0.336 µs | **16.748 ± 0.159 µs** |
| small/config | 27.186 ± 0.184 µs | 19.341 ± 0.255 µs | **7.500 ± 0.088 µs** |
| small/prettierrc | 205.467 ± 1.748 µs | 111.395 ± 0.867 µs | **57.244 ± 0.804 µs** |

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
| generated/deep-nesting | 6.418 ± 0.031 µs | **2.456 ± 0.020 µs** | 3.027 ± 0.032 µs |
| generated/large-array | 18.227 ± 0.200 ms | **7.364 ± 0.099 ms** | 8.668 ± 0.137 ms |
| generated/many-defs | 1.787 ± 0.087 ms | **621.962 ± 9.512 µs** | 745.771 ± 3.844 µs |
| generated/wide-object | 175.876 ± 0.634 µs | 108.213 ± 1.196 µs | **54.049 ± 0.334 µs** |
| large/api-catalog | 562.830 ± 8.144 ns | **240.434 ± 1.041 ns** | 421.797 ± 6.075 ns |
| large/circleci | 2.231 ± 0.012 µs | **464.026 ± 2.714 ns** | 996.317 ± 2.101 ns |
| massive/data-warehouse | 70.402 ± 0.380 µs | **23.973 ± 0.136 µs** | 45.299 ± 0.291 µs |
| massive/k8s-deployment | 522.065 ± 10.952 ns | **208.851 ± 0.936 ns** | 501.404 ± 2.889 ns |
| medium/package-json | 542.303 ± 2.326 ns | **232.423 ± 2.689 ns** | 487.126 ± 2.013 ns |
| medium/user-profile | 851.687 ± 22.525 ns | **370.727 ± 1.324 ns** | 492.184 ± 5.811 ns |
| small/config | 225.780 ± 1.603 ns | **112.117 ± 0.413 ns** | 142.487 ± 3.830 ns |
| small/prettierrc | 725.708 ± 4.353 ns | **173.828 ± 1.433 ns** | 279.470 ± 2.802 ns |

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
| generated/deep-nesting | 7.487 ± 0.062 µs | **2.818 ± 0.026 µs** | 5.230 ± 0.062 µs |
| generated/large-array | 18.211 ± 0.074 ms | **7.412 ± 0.036 ms** | 8.621 ± 0.141 ms |
| generated/many-defs | 1.763 ± 0.018 ms | **628.112 ± 4.696 µs** | 755.664 ± 3.673 µs |
| generated/wide-object | 174.928 ± 2.894 µs | 112.390 ± 3.380 µs | **53.896 ± 0.968 µs** |
| large/api-catalog | 666.531 ± 3.388 ns | **388.820 ± 0.997 ns** | 1.035 ± 0.035 µs |
| large/circleci | **129.056 ± 0.704 ns** | 134.163 ± 2.794 ns | 265.393 ± 0.865 ns |
| massive/data-warehouse | 184.023 ± 1.416 µs | **138.946 ± 1.151 µs** | 271.206 ± 2.483 µs |
| massive/k8s-deployment | 180.683 ± 1.030 ns | **133.249 ± 2.302 ns** | 248.188 ± 1.191 ns |
| medium/package-json | 172.010 ± 1.304 ns | **153.919 ± 0.766 ns** | 240.952 ± 1.244 ns |
| medium/user-profile | 381.772 ± 2.109 ns | **307.393 ± 3.585 ns** | 533.853 ± 3.594 ns |
| small/config | 292.274 ± 4.024 ns | **244.240 ± 1.315 ns** | 541.196 ± 6.875 ns |
| small/prettierrc | **546.122 ± 5.094 ns** | 929.764 ± 29.556 ns | 1.527 ± 0.012 µs |

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
| generated/deep-nesting | **2.978 ± 0.363 ms †** | 109.447 ± 0.997 ms |
| generated/large-array | **1.865 ± 0.059 ms** | 89.054 ± 1.997 ms |
| generated/many-defs | **7.772 ± 0.118 ms** | 437.581 ± 2.353 ms |
| generated/wide-object | **9.244 ± 0.216 ms** | 505.939 ± 5.046 ms |
| large/api-catalog | **2.185 ± 0.057 ms** | 95.904 ± 1.273 ms |
| large/circleci | 4.474 ± 0.099 ms | — |
| massive/data-warehouse | **12.471 ± 0.193 ms** | 794.814 ± 5.397 ms |
| massive/k8s-deployment | **8.343 ± 0.169 ms** | 88.309 ± 4.921 ms |
| medium/package-json | 35.877 ± 0.643 ms | — |
| medium/user-profile | **1.901 ± 0.043 ms** | 88.518 ± 1.538 ms |
| small/config | **1.913 ± 0.030 ms** | 86.254 ± 1.401 ms |
| small/prettierrc | 2.393 ± 0.080 ms | — |

### codegen — peak RSS

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | **4.91 MiB** | 38.00 MiB |
| generated/large-array | **3.73 MiB** | 33.30 ± 0.01 MiB |
| generated/many-defs | **8.80 MiB** | 98.16 MiB |
| generated/wide-object | **11.56 ± 0.15 MiB** | 115.73 ± 0.34 MiB |
| large/api-catalog | **4.25 ± 0.01 MiB** | 34.66 MiB |
| large/circleci | 6.69 MiB | — |
| massive/data-warehouse | **11.50 ± 0.01 MiB** | 176.03 ± 0.51 MiB |
| massive/k8s-deployment | **8.75 MiB** | 31.80 ± 0.01 MiB |
| medium/package-json | 19.73 MiB | — |
| medium/user-profile | **3.81 ± 0.01 MiB** | 33.59 ± 0.01 MiB |
| small/config | **3.58 MiB** | 32.94 ± 0.01 MiB |
| small/prettierrc | 4.61 ± 0.01 MiB | — |

### validate (valid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.457 ± 0.544 ms † | 4.197 ± 0.050 ms | **1.915 ± 0.121 ms** |
| generated/large-array | 58.990 ± 1.045 ms | 44.113 ± 0.308 ms | 44.058 ± 0.377 ms |
| generated/many-defs | 23.038 ± 0.310 ms | 11.980 ± 0.240 ms | **7.084 ± 0.107 ms** |
| generated/wide-object | 28.254 ± 0.662 ms | 8.051 ± 0.162 ms | **4.005 ± 0.140 ms** |
| large/api-catalog | 3.187 ± 0.091 ms | 4.088 ± 0.089 ms | **1.767 ± 0.057 ms** |
| large/circleci | 6.953 ± 0.098 ms | 6.095 ± 0.195 ms | **2.600 ± 0.063 ms** |
| massive/data-warehouse | 49.529 ± 0.430 ms | 11.791 ± 0.081 ms | **5.380 ± 0.108 ms** |
| massive/k8s-deployment | 11.881 ± 0.213 ms | 258.289 ± 28.974 ms † | **2.944 ± 0.114 ms** |
| medium/package-json | 74.532 ± 0.361 ms | — | — |
| medium/user-profile | 3.261 ± 0.064 ms | 3.965 ± 0.110 ms | **1.774 ± 0.047 ms** |
| small/config | 2.933 ± 0.104 ms | 3.887 ± 0.214 ms | **1.658 ± 0.046 ms** |
| small/prettierrc | 3.113 ± 0.064 ms | 3.805 ± 0.298 ms | **1.782 ± 0.057 ms** |

### validate (valid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.97 MiB | 9.86 MiB | **3.97 MiB** |
| generated/large-array | **55.77 ± 0.02 MiB** | 64.69 ± 0.02 MiB | 59.48 ± 0.01 MiB |
| generated/many-defs | 20.64 MiB | 21.78 ± 0.16 MiB | **12.92 ± 0.01 MiB** |
| generated/wide-object | 16.77 ± 0.59 MiB | 15.30 ± 0.02 MiB | **7.19 ± 0.01 MiB** |
| large/api-catalog | 5.89 ± 0.01 MiB | 10.34 ± 0.01 MiB | **3.77 MiB** |
| large/circleci | 9.06 MiB | 13.03 MiB | **5.34 ± 0.01 MiB** |
| massive/data-warehouse | 22.03 ± 0.01 MiB | 21.00 MiB | **9.36 ± 0.00 MiB** |
| massive/k8s-deployment | 11.80 MiB | 20.62 ± 0.07 MiB | **6.00 ± 0.02 MiB** |
| medium/package-json | 27.67 MiB | — | — |
| medium/user-profile | 5.72 ± 0.01 MiB | 10.08 ± 0.01 MiB | **3.64 MiB** |
| small/config | 5.72 MiB | 9.88 MiB | **3.56 ± 0.00 MiB** |
| small/prettierrc | 5.84 MiB | 8.98 MiB | **3.73 ± 0.01 MiB** |

### validate (invalid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.183 ± 0.206 ms | 4.329 ± 0.190 ms | **1.849 ± 0.030 ms** |
| generated/large-array | 59.311 ± 0.746 ms | 44.375 ± 0.332 ms | 44.321 ± 0.404 ms |
| generated/many-defs | 22.757 ± 0.286 ms | 11.429 ± 0.163 ms | **7.239 ± 0.333 ms** |
| generated/wide-object | 28.051 ± 0.192 ms | 8.188 ± 0.141 ms | **3.838 ± 0.117 ms** |
| large/api-catalog | 3.220 ± 0.168 ms | 4.004 ± 0.072 ms | **1.756 ± 0.037 ms** |
| large/circleci | 6.978 ± 0.199 ms | 5.828 ± 0.117 ms | **2.861 ± 0.124 ms** |
| massive/data-warehouse | 50.630 ± 1.262 ms | 11.913 ± 0.115 ms | **5.580 ± 0.133 ms** |
| massive/k8s-deployment | 11.958 ± 0.182 ms | 243.339 ± 39.376 ms † | **3.147 ± 0.167 ms** |
| medium/package-json | 74.760 ± 1.035 ms | — | — |
| medium/user-profile | 3.315 ± 0.146 ms | 3.832 ± 0.198 ms | **1.713 ± 0.046 ms** |
| small/config | 3.074 ± 0.095 ms | 3.776 ± 0.136 ms | **1.661 ± 0.119 ms** |
| small/prettierrc | 3.150 ± 0.201 ms | 3.737 ± 0.078 ms | **1.850 ± 0.076 ms** |

### validate (invalid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.98 ± 0.01 MiB | 10.00 ± 0.02 MiB | **4.11 ± 0.02 MiB** |
| generated/large-array | **55.78 ± 0.00 MiB** | 64.81 ± 0.01 MiB | 59.55 MiB |
| generated/many-defs | 19.09 ± 0.01 MiB | 21.44 ± 0.02 MiB | **12.98 ± 0.00 MiB** |
| generated/wide-object | 15.12 MiB | 15.42 MiB | **7.28 ± 0.01 MiB** |
| large/api-catalog | 5.91 MiB | 10.48 MiB | **3.81 ± 0.01 MiB** |
| large/circleci | 9.06 ± 0.15 MiB | 13.09 MiB | **5.38 ± 0.02 MiB** |
| massive/data-warehouse | 24.20 MiB | 21.05 MiB | **9.59 ± 0.02 MiB** |
| massive/k8s-deployment | 11.81 ± 0.01 MiB | 20.56 ± 0.05 MiB | **6.00 MiB** |
| medium/package-json | 27.67 ± 0.00 MiB | — | — |
| medium/user-profile | 5.73 ± 0.09 MiB | 10.23 ± 0.01 MiB | **3.67 MiB** |
| small/config | 5.75 MiB | 10.00 ± 0.01 MiB | **3.64 ± 0.01 MiB** |
| small/prettierrc | 5.84 ± 0.02 MiB | 9.16 MiB | **3.80 MiB** |

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
