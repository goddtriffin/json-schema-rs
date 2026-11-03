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
| generated/deep-nesting | **1.882 ± 0.016 ms** | 9.671 ± 0.060 ms |
| generated/large-array | **103.232 ± 0.127 µs** | 1.034 ± 0.004 ms |
| generated/many-defs | **14.191 ± 0.018 ms** | 122.217 ± 2.982 ms |
| generated/wide-object | **13.942 ± 0.014 ms** | 147.884 ± 0.229 ms |
| large/api-catalog | **379.414 ± 0.944 µs** | 3.110 ± 0.028 ms |
| large/circleci | 6.722 ± 0.020 ms | — |
| massive/data-warehouse | **28.988 ± 0.303 ms** | 269.572 ± 3.424 ms |
| massive/k8s-deployment | 15.328 ± 0.016 ms | **1.791 ± 0.006 ms** |
| medium/package-json | 43.262 ± 0.070 ms | — |
| medium/user-profile | **141.250 ± 0.181 µs** | 1.425 ± 0.019 ms |
| small/config | **52.385 ± 0.038 µs** | 512.216 ± 0.555 µs |
| small/prettierrc | 850.041 ± 14.707 µs | — |

### codegen — peak heap

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | **1001.39 KiB** | 12.65 MiB |
| generated/large-array | **70.24 KiB** | 1.23 MiB |
| generated/many-defs | **9.21 MiB** | 133.38 MiB |
| generated/wide-object | **8.69 MiB** | 162.36 MiB |
| large/api-catalog | **245.22 KiB** | 3.64 MiB |
| large/circleci | 3.89 MiB | — |
| massive/data-warehouse | **14.96 MiB** | 288.90 MiB |
| massive/k8s-deployment | 8.34 MiB | **2.79 MiB** |
| medium/package-json | 32.65 MiB | — |
| medium/user-profile | **97.24 KiB** | 1.71 MiB |
| small/config | **36.49 KiB** | 639.58 KiB |
| small/prettierrc | 528.44 KiB | — |

### schema compile — time

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 1.029 ± 0.003 ms | 205.712 ± 0.206 µs | **100.016 ± 0.115 µs** |
| generated/large-array | 44.479 ± 0.044 µs | 20.685 ± 0.057 µs | **10.588 ± 0.026 µs** |
| generated/many-defs | 13.757 ± 0.077 ms | 3.002 ± 0.006 ms | **1.625 ± 0.001 ms** |
| generated/wide-object | 23.140 ± 0.124 ms | 2.522 ± 0.004 ms | **1.304 ± 0.002 ms** |
| large/api-catalog | 177.638 ± 0.330 µs | 103.125 ± 0.084 µs | **43.505 ± 0.095 µs** |
| large/circleci | 2.865 ± 0.003 ms | 1.016 ± 0.004 ms | **508.686 ± 0.683 µs** |
| massive/data-warehouse | 44.679 ± 0.091 ms | 6.274 ± 0.008 ms | **2.626 ± 0.020 ms** |
| massive/k8s-deployment | 7.324 ± 0.031 ms | 1.585 ± 0.006 ms | **742.271 ± 0.372 µs** |
| medium/package-json | 68.500 ± 0.428 ms | 13.995 ± 0.060 ms | **5.837 ± 0.033 ms** |
| medium/user-profile | 66.669 ± 0.096 µs | 39.944 ± 0.424 µs | **16.430 ± 0.030 µs** |
| small/config | 27.104 ± 0.215 µs | 18.921 ± 0.030 µs | **7.470 ± 0.043 µs** |
| small/prettierrc | 203.947 ± 1.262 µs | 110.192 ± 0.287 µs | **56.469 ± 0.215 µs** |

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
| generated/deep-nesting | 6.361 ± 0.093 µs | **2.518 ± 0.009 µs** | 3.002 ± 0.034 µs |
| generated/large-array | 17.927 ± 0.223 ms | **7.490 ± 0.240 ms** | 9.284 ± 0.373 ms |
| generated/many-defs | 1.712 ± 0.009 ms | **627.026 ± 1.863 µs** | 736.961 ± 2.316 µs |
| generated/wide-object | 171.480 ± 0.269 µs | 107.690 ± 1.049 µs | **55.873 ± 0.466 µs** |
| large/api-catalog | 559.585 ± 1.426 ns | **238.795 ± 0.907 ns** | 416.130 ± 2.615 ns |
| large/circleci | 2.236 ± 0.018 µs | **473.990 ± 3.226 ns** | 1.016 ± 0.010 µs |
| massive/data-warehouse | 69.006 ± 0.697 µs | **24.037 ± 0.032 µs** | 45.063 ± 0.242 µs |
| massive/k8s-deployment | 514.686 ± 2.029 ns | **215.711 ± 1.240 ns** | 499.819 ± 4.762 ns |
| medium/package-json | 533.981 ± 0.875 ns | **231.607 ± 1.047 ns** | 484.283 ± 1.845 ns |
| medium/user-profile | 843.079 ± 1.462 ns | **378.211 ± 1.102 ns** | 490.761 ± 3.522 ns |
| small/config | 222.962 ± 0.338 ns | **114.727 ± 0.713 ns** | 140.546 ± 2.417 ns |
| small/prettierrc | 713.395 ± 1.877 ns | **180.974 ± 0.956 ns** | 280.844 ± 2.145 ns |

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
| generated/deep-nesting | 7.496 ± 0.009 µs | **2.861 ± 0.018 µs** | 5.121 ± 0.025 µs |
| generated/large-array | 17.977 ± 0.087 ms | **7.450 ± 0.057 ms** | 8.698 ± 0.095 ms |
| generated/many-defs | 1.731 ± 0.005 ms | **626.712 ± 6.920 µs** | 743.697 ± 1.472 µs |
| generated/wide-object | 169.722 ± 0.367 µs | 109.339 ± 0.333 µs | **54.740 ± 0.060 µs** |
| large/api-catalog | 655.551 ± 0.829 ns | **402.492 ± 1.439 ns** | 1.031 ± 0.008 µs |
| large/circleci | 130.981 ± 0.756 ns | 129.177 ± 0.941 ns | 265.113 ± 1.203 ns |
| massive/data-warehouse | 177.836 ± 1.463 µs | **138.098 ± 1.835 µs** | 272.003 ± 1.423 µs |
| massive/k8s-deployment | 176.800 ± 0.476 ns | **134.783 ± 0.311 ns** | 241.033 ± 1.293 ns |
| medium/package-json | 173.594 ± 0.447 ns | **150.996 ± 0.375 ns** | 240.114 ± 0.820 ns |
| medium/user-profile | 370.144 ± 5.674 ns | **321.959 ± 1.119 ns** | 531.393 ± 4.616 ns |
| small/config | 290.224 ± 0.793 ns | **250.890 ± 1.809 ns** | 526.623 ± 2.823 ns |
| small/prettierrc | **522.838 ± 1.889 ns** | 918.039 ± 4.948 ns | 1.532 ± 0.005 µs |

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
| generated/deep-nesting | **4.703 ± 0.493 ms †** | 108.557 ± 0.562 ms |
| generated/large-array | **2.041 ± 0.036 ms** | 87.508 ± 0.415 ms |
| generated/many-defs | **20.292 ± 0.240 ms** | 436.226 ± 1.710 ms |
| generated/wide-object | **19.240 ± 0.325 ms** | 504.076 ± 1.154 ms |
| large/api-catalog | **2.583 ± 0.045 ms** | 94.357 ± 1.487 ms |
| large/circleci | 10.757 ± 0.097 ms | — |
| massive/data-warehouse | **35.546 ± 0.566 ms** | 784.101 ± 2.212 ms |
| massive/k8s-deployment | **20.973 ± 0.120 ms** | 86.370 ± 0.407 ms |
| medium/package-json | 96.062 ± 0.709 ms | — |
| medium/user-profile | **2.108 ± 0.044 ms** | 87.411 ± 0.521 ms |
| small/config | **1.899 ± 0.052 ms** | 84.603 ± 0.876 ms |
| small/prettierrc | 3.182 ± 0.093 ms | — |

### codegen — peak RSS

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | **6.94 ± 0.03 MiB** | 38.00 ± 0.02 MiB |
| generated/large-array | **4.42 ± 0.01 MiB** | 33.31 ± 0.01 MiB |
| generated/many-defs | **18.98 ± 0.06 MiB** | 97.95 ± 0.01 MiB |
| generated/wide-object | **20.42 MiB** | 113.89 ± 0.01 MiB |
| large/api-catalog | **5.12 ± 0.02 MiB** | 34.66 ± 0.01 MiB |
| large/circleci | 11.00 ± 0.04 MiB | — |
| massive/data-warehouse | **27.48 MiB** | 175.92 ± 0.04 MiB |
| massive/k8s-deployment | **17.66 ± 0.03 MiB** | 31.78 ± 0.02 MiB |
| medium/package-json | 38.52 ± 0.04 MiB | — |
| medium/user-profile | **4.62 MiB** | 33.62 ± 0.03 MiB |
| small/config | **4.22 ± 0.01 MiB** | 32.92 MiB |
| small/prettierrc | 5.64 ± 0.01 MiB | — |

### validate (valid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.439 ± 0.456 ms † | 4.089 ± 0.055 ms | **1.813 ± 0.025 ms** |
| generated/large-array | 58.508 ± 0.422 ms | 45.138 ± 1.078 ms | 43.986 ± 0.305 ms |
| generated/many-defs | 23.051 ± 0.357 ms | 11.326 ± 0.124 ms | **7.311 ± 0.062 ms** |
| generated/wide-object | 27.757 ± 0.439 ms | 8.153 ± 0.080 ms | **3.873 ± 0.057 ms** |
| large/api-catalog | 3.183 ± 0.153 ms | 3.902 ± 0.054 ms | **1.737 ± 0.030 ms** |
| large/circleci | 6.810 ± 0.037 ms | 5.695 ± 0.064 ms | **2.559 ± 0.033 ms** |
| massive/data-warehouse | 49.561 ± 0.289 ms | 11.867 ± 0.052 ms | **5.309 ± 0.064 ms** |
| massive/k8s-deployment | 11.833 ± 0.051 ms | 335.033 ± 39.553 ms † | **2.883 ± 0.048 ms** |
| medium/package-json | 74.297 ± 1.741 ms | — | **7.509 ± 0.107 ms** |
| medium/user-profile | 2.902 ± 0.082 ms | 3.690 ± 0.116 ms | **1.623 ± 0.033 ms** |
| small/config | 2.859 ± 0.027 ms | 3.597 ± 0.097 ms | **1.615 ± 0.029 ms** |
| small/prettierrc | 3.086 ± 0.027 ms | 3.726 ± 0.059 ms | **1.723 ± 0.033 ms** |

### validate (valid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.97 MiB | 9.86 ± 0.02 MiB | **4.12 MiB** |
| generated/large-array | **55.77 ± 0.01 MiB** | 64.67 MiB | 59.64 MiB |
| generated/many-defs | 20.69 MiB | 21.38 MiB | **13.22 ± 0.01 MiB** |
| generated/wide-object | 15.11 ± 0.02 MiB | 15.30 MiB | **7.33 MiB** |
| large/api-catalog | 5.89 ± 0.02 MiB | 10.36 MiB | **3.94 MiB** |
| large/circleci | 9.08 MiB | 13.11 ± 0.03 MiB | **5.44 MiB** |
| massive/data-warehouse | 22.00 ± 0.01 MiB | 20.95 ± 0.00 MiB | **9.44 ± 0.02 MiB** |
| massive/k8s-deployment | 11.80 MiB | 20.53 ± 0.03 MiB | **6.06 ± 0.00 MiB** |
| medium/package-json | 27.69 ± 0.34 MiB | — | **11.56 ± 0.02 MiB** |
| medium/user-profile | 5.72 ± 0.01 MiB | 10.08 MiB | **3.80 MiB** |
| small/config | 5.72 MiB | 9.84 ± 0.01 MiB | **3.69 MiB** |
| small/prettierrc | 5.84 MiB | 8.95 MiB | **3.89 MiB** |

### validate (invalid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.137 ± 0.028 ms | 4.119 ± 0.050 ms | **1.813 ± 0.037 ms** |
| generated/large-array | 58.417 ± 0.478 ms | 44.816 ± 1.127 ms | 43.760 ± 0.434 ms |
| generated/many-defs | 23.034 ± 0.242 ms | 11.355 ± 0.065 ms | **7.362 ± 0.070 ms** |
| generated/wide-object | 27.762 ± 0.220 ms | 8.055 ± 0.166 ms | **3.865 ± 0.029 ms** |
| large/api-catalog | 3.135 ± 0.150 ms | 3.946 ± 0.047 ms | **1.731 ± 0.041 ms** |
| large/circleci | 6.809 ± 0.052 ms | 5.647 ± 0.076 ms | **2.545 ± 0.026 ms** |
| massive/data-warehouse | 51.054 ± 0.338 ms | 11.898 ± 0.122 ms | **5.448 ± 0.056 ms** |
| massive/k8s-deployment | 11.824 ± 0.072 ms | 292.349 ± 46.029 ms † | **2.916 ± 0.043 ms** |
| medium/package-json | 74.123 ± 0.338 ms | — | **7.549 ± 0.098 ms** |
| medium/user-profile | 2.905 ± 0.043 ms | 3.704 ± 0.067 ms | **1.657 ± 0.092 ms** |
| small/config | 2.827 ± 0.025 ms | 3.602 ± 0.046 ms | **1.614 ± 0.021 ms** |
| small/prettierrc | 3.083 ± 0.035 ms | 3.872 ± 0.235 ms | **1.723 ± 0.043 ms** |

### validate (invalid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.98 MiB | 10.02 MiB | **4.17 MiB** |
| generated/large-array | **55.81 ± 0.02 MiB** | 64.80 ± 0.02 MiB | 59.67 ± 0.00 MiB |
| generated/many-defs | 19.09 ± 0.01 MiB | 21.42 ± 0.02 MiB | **13.28 ± 0.05 MiB** |
| generated/wide-object | 15.12 ± 0.01 MiB | 15.42 MiB | **7.38 ± 0.01 MiB** |
| large/api-catalog | 5.91 MiB | 10.48 MiB | **3.97 ± 0.01 MiB** |
| large/circleci | 9.08 ± 0.01 MiB | 13.03 ± 0.00 MiB | **5.50 ± 0.00 MiB** |
| massive/data-warehouse | 24.27 ± 0.46 MiB | 21.11 MiB | **9.55 ± 0.01 MiB** |
| massive/k8s-deployment | 11.81 MiB | 20.69 ± 0.08 MiB | **6.06 MiB** |
| medium/package-json | 27.67 ± 0.01 MiB | — | **11.55 ± 0.01 MiB** |
| medium/user-profile | 5.73 ± 0.01 MiB | 10.20 ± 0.00 MiB | **3.80 MiB** |
| small/config | 5.75 MiB | 10.00 ± 0.01 MiB | **3.77 MiB** |
| small/prettierrc | 5.86 MiB | 9.12 MiB | **3.92 MiB** |

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
