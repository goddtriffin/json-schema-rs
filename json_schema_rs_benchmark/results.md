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
| CLI | validate (valid) | time | jsonschemars | 10 / 12 |
| CLI | validate (valid) | peak RSS | jsonschemars | 11 / 12 |
| CLI | validate (invalid) | time | jsonschemars | 11 / 12 |
| CLI | validate (invalid) | peak RSS | jsonschemars | 11 / 12 |

## In-process (library calls)

### codegen — time

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | **373.448 ± 2.266 µs** | 9.550 ± 0.010 ms |
| generated/large-array | **44.959 ± 0.143 µs** | 1.023 ± 0.001 ms |
| generated/many-defs | **4.390 ± 0.009 ms** | 119.275 ± 0.479 ms |
| generated/wide-object | **5.981 ± 0.327 ms** | 145.631 ± 0.401 ms |
| large/api-catalog | **165.057 ± 0.477 µs** | 3.064 ± 0.021 ms |
| large/circleci | 1.666 ± 0.002 ms | — |
| massive/data-warehouse | **8.444 ± 0.281 ms** | 266.839 ± 0.870 ms |
| massive/k8s-deployment | 4.623 ± 0.006 ms | **1.798 ± 0.005 ms** |
| medium/package-json | 16.063 ± 0.241 ms | — |
| medium/user-profile | **60.497 ± 0.066 µs** | 1.412 ± 0.026 ms |
| small/config | **20.545 ± 0.020 µs** | 507.349 ± 25.768 µs |
| small/prettierrc | 333.547 ± 4.345 µs | — |

### codegen — peak heap

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | **343.19 KiB** | 12.65 MiB |
| generated/large-array | **31.29 KiB** | 1.23 MiB |
| generated/many-defs | **3.28 MiB** | 133.38 MiB |
| generated/wide-object | **4.66 MiB** | 162.36 MiB |
| large/api-catalog | **96.96 KiB** | 3.64 MiB |
| large/circleci | 1.55 MiB | — |
| massive/data-warehouse | **5.65 MiB** | 288.90 MiB |
| massive/k8s-deployment | 3.42 MiB | **2.79 MiB** |
| medium/package-json | 15.65 MiB | — |
| medium/user-profile | **41.86 KiB** | 1.71 MiB |
| small/config | **16.29 KiB** | 639.58 KiB |
| small/prettierrc | 181.25 KiB | — |

### schema compile — time

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 1.026 ± 0.002 ms | 203.954 ± 0.301 µs | **99.638 ± 1.087 µs** |
| generated/large-array | 44.259 ± 0.036 µs | 20.341 ± 0.024 µs | **10.574 ± 0.015 µs** |
| generated/many-defs | 13.608 ± 0.037 ms | 2.971 ± 0.002 ms | **1.623 ± 0.019 ms** |
| generated/wide-object | 23.096 ± 0.220 ms | 2.502 ± 0.022 ms | **1.294 ± 0.002 ms** |
| large/api-catalog | 175.690 ± 0.199 µs | 100.902 ± 0.124 µs | **43.314 ± 0.026 µs** |
| large/circleci | 2.849 ± 0.023 ms | 1.010 ± 0.003 ms | **505.078 ± 2.458 µs** |
| massive/data-warehouse | 44.008 ± 0.072 ms | 6.224 ± 0.017 ms | **2.603 ± 0.003 ms** |
| massive/k8s-deployment | 7.312 ± 0.006 ms | 1.584 ± 0.005 ms | **738.853 ± 0.911 µs** |
| medium/package-json | 68.257 ± 0.553 ms | 13.966 ± 0.146 ms | **5.777 ± 0.010 ms** |
| medium/user-profile | 66.177 ± 0.530 µs | 39.606 ± 0.293 µs | **16.128 ± 0.022 µs** |
| small/config | 26.948 ± 0.334 µs | 18.975 ± 0.108 µs | **7.298 ± 0.072 µs** |
| small/prettierrc | 203.580 ± 0.424 µs | 109.620 ± 0.089 µs | **55.934 ± 0.420 µs** |

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
| generated/deep-nesting | 6.793 ± 0.009 µs | **2.407 ± 0.019 µs** | 3.055 ± 0.014 µs |
| generated/large-array | 17.937 ± 0.105 ms | **7.289 ± 0.051 ms** | 8.659 ± 0.082 ms |
| generated/many-defs | 1.759 ± 0.024 ms | **618.120 ± 1.530 µs** | 752.898 ± 4.748 µs |
| generated/wide-object | 169.046 ± 0.443 µs | 109.132 ± 0.446 µs | **54.822 ± 0.194 µs** |
| large/api-catalog | 558.280 ± 4.906 ns | **239.930 ± 3.574 ns** | 424.148 ± 6.894 ns |
| large/circleci | 2.405 ± 0.012 µs | **464.730 ± 3.203 ns** | 1.002 ± 0.007 µs |
| massive/data-warehouse | 68.981 ± 0.238 µs | **23.934 ± 0.172 µs** | 44.430 ± 0.256 µs |
| massive/k8s-deployment | 514.297 ± 1.414 ns | **210.162 ± 1.128 ns** | 498.540 ± 1.509 ns |
| medium/package-json | 533.729 ± 1.357 ns | **225.727 ± 0.614 ns** | 485.571 ± 0.313 ns |
| medium/user-profile | 840.751 ± 1.078 ns | **369.373 ± 3.375 ns** | 494.585 ± 2.016 ns |
| small/config | 221.866 ± 0.315 ns | **111.457 ± 0.162 ns** | 142.097 ± 1.432 ns |
| small/prettierrc | 715.583 ± 5.366 ns | **171.141 ± 1.575 ns** | 275.699 ± 0.706 ns |

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
| generated/deep-nesting | 7.540 ± 0.011 µs | **2.755 ± 0.009 µs** | 5.142 ± 0.011 µs |
| generated/large-array | 17.840 ± 0.051 ms | **7.247 ± 0.022 ms** | 8.667 ± 0.065 ms |
| generated/many-defs | 1.756 ± 0.007 ms | **618.522 ± 0.930 µs** | 748.303 ± 4.995 µs |
| generated/wide-object | 173.554 ± 1.307 µs | 109.466 ± 1.282 µs | **53.759 ± 0.924 µs** |
| large/api-catalog | 657.156 ± 3.998 ns | **390.094 ± 1.678 ns** | 1.035 ± 0.009 µs |
| large/circleci | 126.050 ± 0.304 ns | 127.456 ± 0.406 ns | 266.463 ± 1.188 ns |
| massive/data-warehouse | 178.605 ± 0.166 µs | **137.909 ± 0.186 µs** | 272.875 ± 1.384 µs |
| massive/k8s-deployment | 175.101 ± 0.339 ns | **133.948 ± 0.449 ns** | 243.322 ± 2.524 ns |
| medium/package-json | 170.430 ± 0.118 ns | **150.990 ± 1.667 ns** | 242.910 ± 1.032 ns |
| medium/user-profile | 364.708 ± 0.874 ns | **307.547 ± 2.227 ns** | 533.375 ± 3.037 ns |
| small/config | 286.851 ± 0.765 ns | **242.899 ± 0.674 ns** | 533.460 ± 3.430 ns |
| small/prettierrc | **535.119 ± 0.990 ns** | 901.211 ± 4.232 ns | 1.529 ± 0.009 µs |

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
| generated/deep-nesting | **7.423 ± 0.485 ms** | 130.049 ± 0.890 ms |
| generated/large-array | **2.882 ± 0.035 ms** | 109.226 ± 1.164 ms |
| generated/many-defs | **9.824 ± 0.298 ms** | 457.190 ± 2.174 ms |
| generated/wide-object | **10.941 ± 0.331 ms** | 527.382 ± 2.952 ms |
| large/api-catalog | **3.060 ± 0.043 ms** | 116.235 ± 1.667 ms |
| large/circleci | 5.145 ± 0.071 ms | — |
| massive/data-warehouse | **14.419 ± 0.080 ms** | 806.794 ± 2.964 ms |
| massive/k8s-deployment | **10.337 ± 0.685 ms** | 107.573 ± 2.032 ms |
| medium/package-json | 42.504 ± 0.550 ms | — |
| medium/user-profile | **2.844 ± 0.052 ms** | 110.287 ± 2.509 ms |
| small/config | **2.708 ± 0.032 ms** | 106.400 ± 1.516 ms |
| small/prettierrc | 3.304 ± 0.699 ms † | — |

### codegen — peak RSS

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | **5.03 ± 0.03 MiB** | 38.02 ± 0.01 MiB |
| generated/large-array | **3.92 ± 0.03 MiB** | 33.31 ± 0.02 MiB |
| generated/many-defs | **8.81 ± 0.01 MiB** | 98.05 ± 0.03 MiB |
| generated/wide-object | **11.11 ± 0.02 MiB** | 114.62 ± 0.40 MiB |
| large/api-catalog | **4.31 MiB** | 34.69 ± 0.01 MiB |
| large/circleci | 6.78 ± 0.01 MiB | — |
| massive/data-warehouse | **11.61 ± 0.04 MiB** | 175.86 ± 0.11 MiB |
| massive/k8s-deployment | **8.98 ± 0.14 MiB** | 31.81 MiB |
| medium/package-json | 19.58 MiB | — |
| medium/user-profile | **3.98 ± 0.01 MiB** | 33.61 MiB |
| small/config | **3.66 ± 0.01 MiB** | 32.94 MiB |
| small/prettierrc | 4.70 ± 0.01 MiB | — |

### validate (valid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 9.083 ± 0.382 ms | 5.125 ± 0.092 ms | **2.929 ± 0.102 ms** |
| generated/large-array | 60.309 ± 0.548 ms | 46.902 ± 1.090 ms | 46.510 ± 0.479 ms |
| generated/many-defs | 25.147 ± 0.548 ms | 12.038 ± 0.163 ms | **9.588 ± 0.074 ms** |
| generated/wide-object | 29.747 ± 0.646 ms | 8.810 ± 0.132 ms | **4.950 ± 0.077 ms** |
| large/api-catalog | 4.347 ± 0.064 ms | 4.921 ± 0.065 ms | **3.146 ± 0.284 ms** |
| large/circleci | 8.935 ± 0.390 ms | 6.333 ± 0.079 ms | **3.584 ± 0.046 ms** |
| massive/data-warehouse | 51.811 ± 0.341 ms | 12.610 ± 0.255 ms | **7.730 ± 0.192 ms** |
| massive/k8s-deployment | 13.893 ± 0.311 ms | 402.604 ± 41.548 ms † | **3.957 ± 0.068 ms** |
| medium/package-json | 82.743 ± 0.663 ms | — | **15.728 ± 1.087 ms** |
| medium/user-profile | **3.916 ± 0.065 ms** | 4.954 ± 0.073 ms | 4.315 ± 0.077 ms |
| small/config | 3.892 ± 0.056 ms | 4.431 ± 0.106 ms | **2.739 ± 0.158 ms** |
| small/prettierrc | 4.803 ± 0.670 ms † | 4.537 ± 0.108 ms | **2.818 ± 0.087 ms** |

### validate (valid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.97 ± 0.01 MiB | 9.91 MiB | **4.00 ± 0.00 MiB** |
| generated/large-array | **55.78 MiB** | 64.67 ± 0.01 MiB | 59.58 MiB |
| generated/many-defs | 19.06 ± 0.01 MiB | 21.36 MiB | **12.94 ± 0.01 MiB** |
| generated/wide-object | 15.09 ± 0.01 MiB | 15.28 MiB | **7.19 MiB** |
| large/api-catalog | 5.88 ± 0.00 MiB | 10.36 ± 0.01 MiB | **3.80 ± 0.01 MiB** |
| large/circleci | 9.08 MiB | 13.05 ± 0.00 MiB | **5.30 ± 0.01 MiB** |
| massive/data-warehouse | 21.98 ± 0.03 MiB | 20.98 MiB | **9.25 MiB** |
| massive/k8s-deployment | 11.80 MiB | 20.48 MiB | **5.94 ± 0.02 MiB** |
| medium/package-json | 28.14 MiB | — | **11.39 MiB** |
| medium/user-profile | 5.72 ± 0.01 MiB | 10.08 MiB | **3.67 ± 0.01 MiB** |
| small/config | 5.72 ± 0.02 MiB | 9.84 ± 0.01 MiB | **3.56 MiB** |
| small/prettierrc | 5.83 ± 0.01 MiB | 8.97 ± 0.02 MiB | **3.77 MiB** |

### validate (invalid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 7.820 ± 0.536 ms | 5.069 ± 0.051 ms | **3.054 ± 0.037 ms** |
| generated/large-array | 60.226 ± 0.203 ms | 46.909 ± 0.516 ms | 46.658 ± 0.543 ms |
| generated/many-defs | 25.153 ± 0.347 ms | 12.076 ± 0.110 ms | **9.644 ± 0.392 ms** |
| generated/wide-object | 29.824 ± 0.232 ms | 8.718 ± 0.080 ms | **5.000 ± 0.149 ms** |
| large/api-catalog | 4.158 ± 0.061 ms | 5.053 ± 0.059 ms | **3.015 ± 0.059 ms** |
| large/circleci | 9.062 ± 0.153 ms | 6.478 ± 0.160 ms | **3.563 ± 0.061 ms** |
| massive/data-warehouse | 52.977 ± 0.295 ms | 12.653 ± 0.138 ms | **7.845 ± 0.501 ms** |
| massive/k8s-deployment | 14.234 ± 0.642 ms | 364.093 ± 36.530 ms † | **3.951 ± 0.061 ms** |
| medium/package-json | 82.946 ± 1.294 ms | — | **15.651 ± 0.804 ms** |
| medium/user-profile | 4.060 ± 0.046 ms | 4.693 ± 0.062 ms | **3.374 ± 0.262 ms** |
| small/config | 3.818 ± 0.039 ms | 4.675 ± 0.121 ms | **2.688 ± 0.097 ms** |
| small/prettierrc | 4.047 ± 0.052 ms | 4.457 ± 0.054 ms | **2.766 ± 0.050 ms** |

### validate (invalid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.98 MiB | 10.00 ± 0.01 MiB | **4.12 ± 0.01 MiB** |
| generated/large-array | **55.81 MiB** | 64.78 ± 0.01 MiB | 59.64 MiB |
| generated/many-defs | 19.09 ± 0.01 MiB | 21.44 ± 0.02 MiB | **13.06 ± 0.01 MiB** |
| generated/wide-object | 15.12 MiB | 15.42 MiB | **7.28 ± 0.01 MiB** |
| large/api-catalog | 5.89 ± 0.01 MiB | 10.50 MiB | **3.86 MiB** |
| large/circleci | 9.08 MiB | 13.11 MiB | **5.36 ± 0.00 MiB** |
| massive/data-warehouse | 23.17 ± 0.31 MiB | 21.09 ± 0.02 MiB | **9.42 MiB** |
| massive/k8s-deployment | 12.23 ± 0.17 MiB | 20.59 MiB | **5.98 ± 0.01 MiB** |
| medium/package-json | 28.62 ± 0.48 MiB | — | **11.41 MiB** |
| medium/user-profile | 5.73 MiB | 10.20 ± 0.01 MiB | **3.70 ± 0.01 MiB** |
| small/config | 5.73 ± 0.01 MiB | 10.02 MiB | **3.69 MiB** |
| small/prettierrc | 5.86 MiB | 9.16 ± 0.02 MiB | **3.83 ± 0.01 MiB** |

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
