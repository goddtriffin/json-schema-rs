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
| generated/deep-nesting | **3.019 ± 0.006 ms** | 9.593 ± 0.034 ms |
| generated/large-array | **131.997 ± 0.296 µs** | 1.021 ± 0.001 ms |
| generated/many-defs | **19.873 ± 0.086 ms** | 121.055 ± 1.903 ms |
| generated/wide-object | **17.543 ± 0.028 ms** | 146.758 ± 0.714 ms |
| large/api-catalog | **455.322 ± 0.402 µs** | 3.083 ± 0.029 ms |
| large/circleci | 6.802 ± 0.009 ms | — |
| massive/data-warehouse | **36.234 ± 0.342 ms** | 267.859 ± 1.379 ms |
| massive/k8s-deployment | 15.420 ± 0.026 ms | **1.795 ± 0.001 ms** |
| medium/package-json | 43.431 ± 0.231 ms | — |
| medium/user-profile | **184.642 ± 0.127 µs** | 1.416 ± 0.021 ms |
| small/config | **63.763 ± 0.050 µs** | 505.987 ± 1.063 µs |
| small/prettierrc | 852.973 ± 2.099 µs | — |

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
| generated/deep-nesting | 1.020 ± 0.003 ms | 208.917 ± 0.264 µs | **100.178 ± 0.157 µs** |
| generated/large-array | 44.494 ± 0.058 µs | 20.895 ± 0.022 µs | **10.636 ± 0.024 µs** |
| generated/many-defs | 13.855 ± 0.018 ms | 3.023 ± 0.005 ms | **1.629 ± 0.002 ms** |
| generated/wide-object | 23.325 ± 0.027 ms | 2.528 ± 0.003 ms | **1.301 ± 0.011 ms** |
| large/api-catalog | 178.145 ± 0.120 µs | 102.987 ± 0.130 µs | **43.615 ± 0.041 µs** |
| large/circleci | 2.855 ± 0.009 ms | 1.019 ± 0.001 ms | **507.905 ± 1.588 µs** |
| massive/data-warehouse | 44.830 ± 0.578 ms | 6.343 ± 0.018 ms | **2.620 ± 0.002 ms** |
| massive/k8s-deployment | 7.381 ± 0.105 ms | 1.604 ± 0.022 ms | **746.949 ± 1.421 µs** |
| medium/package-json | 68.300 ± 0.131 ms | 13.984 ± 0.028 ms | **5.813 ± 0.041 ms** |
| medium/user-profile | 66.847 ± 0.085 µs | 39.910 ± 0.173 µs | **16.401 ± 0.014 µs** |
| small/config | 26.921 ± 0.092 µs | 19.124 ± 0.027 µs | **7.420 ± 0.063 µs** |
| small/prettierrc | 204.514 ± 1.233 µs | 110.486 ± 0.158 µs | **56.583 ± 0.094 µs** |

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
| generated/deep-nesting | 6.681 ± 0.007 µs | **2.512 ± 0.031 µs** | 3.016 ± 0.024 µs |
| generated/large-array | 17.812 ± 0.117 ms | **7.544 ± 0.079 ms** | 8.520 ± 0.057 ms |
| generated/many-defs | 1.691 ± 0.003 ms | **619.599 ± 1.442 µs** | 737.253 ± 8.082 µs |
| generated/wide-object | 171.945 ± 1.105 µs | 110.157 ± 0.427 µs | **54.030 ± 0.768 µs** |
| large/api-catalog | 561.063 ± 1.904 ns | **235.977 ± 1.648 ns** | 419.972 ± 2.020 ns |
| large/circleci | 2.219 ± 0.010 µs | **483.260 ± 2.789 ns** | 1.003 ± 0.011 µs |
| massive/data-warehouse | 69.242 ± 0.174 µs | **24.151 ± 0.098 µs** | 45.087 ± 0.423 µs |
| massive/k8s-deployment | 514.668 ± 0.521 ns | **215.744 ± 0.754 ns** | 503.702 ± 4.199 ns |
| medium/package-json | 536.422 ± 0.737 ns | **235.340 ± 1.951 ns** | 484.691 ± 2.067 ns |
| medium/user-profile | 836.874 ± 1.277 ns | **381.307 ± 2.579 ns** | 493.202 ± 3.632 ns |
| small/config | 224.660 ± 0.564 ns | **115.830 ± 0.523 ns** | 141.358 ± 3.002 ns |
| small/prettierrc | 718.493 ± 2.724 ns | **177.605 ± 0.677 ns** | 282.664 ± 2.046 ns |

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
| generated/deep-nesting | 7.663 ± 0.069 µs | **2.898 ± 0.017 µs** | 5.146 ± 0.020 µs |
| generated/large-array | 17.909 ± 0.064 ms | **7.461 ± 0.042 ms** | 8.547 ± 0.073 ms |
| generated/many-defs | 1.730 ± 0.008 ms | **618.796 ± 2.156 µs** | 747.560 ± 1.920 µs |
| generated/wide-object | 169.197 ± 0.450 µs | 111.434 ± 1.723 µs | **54.431 ± 0.122 µs** |
| large/api-catalog | 654.539 ± 2.925 ns | **396.022 ± 0.586 ns** | 1.028 ± 0.010 µs |
| large/circleci | **126.146 ± 0.114 ns** | 137.526 ± 0.682 ns | 266.109 ± 0.890 ns |
| massive/data-warehouse | 179.056 ± 0.431 µs | **137.912 ± 0.954 µs** | 270.435 ± 2.748 µs |
| massive/k8s-deployment | 173.126 ± 0.248 ns | **134.508 ± 0.584 ns** | 241.286 ± 1.008 ns |
| medium/package-json | 172.268 ± 0.215 ns | **152.608 ± 0.672 ns** | 240.147 ± 1.005 ns |
| medium/user-profile | 370.951 ± 1.335 ns | **317.808 ± 1.208 ns** | 535.185 ± 4.048 ns |
| small/config | 287.577 ± 1.265 ns | **251.973 ± 0.804 ns** | 553.013 ± 11.314 ns |
| small/prettierrc | **534.911 ± 1.028 ns** | 909.008 ± 2.076 ns | 1.516 ± 0.006 µs |

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
| generated/deep-nesting | **5.978 ± 0.534 ms** | 106.542 ± 0.827 ms |
| generated/large-array | **2.044 ± 0.065 ms** | 85.341 ± 0.361 ms |
| generated/many-defs | **27.456 ± 0.150 ms** | 432.568 ± 1.513 ms |
| generated/wide-object | **24.110 ± 0.153 ms** | 502.630 ± 2.077 ms |
| large/api-catalog | **2.561 ± 0.051 ms** | 92.155 ± 0.977 ms |
| large/circleci | 10.690 ± 0.081 ms | — |
| massive/data-warehouse | **42.776 ± 0.145 ms** | 782.149 ± 4.027 ms |
| massive/k8s-deployment | **20.806 ± 0.100 ms** | 83.999 ± 0.466 ms |
| medium/package-json | 95.862 ± 1.732 ms | — |
| medium/user-profile | **2.165 ± 0.062 ms** | 85.136 ± 0.645 ms |
| small/config | **1.901 ± 0.032 ms** | 82.690 ± 1.475 ms |
| small/prettierrc | 3.129 ± 0.144 ms | — |

### codegen — peak RSS

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | **7.56 ± 0.01 MiB** | 38.02 ± 0.01 MiB |
| generated/large-array | **4.61 ± 0.01 MiB** | 33.30 MiB |
| generated/many-defs | **21.91 ± 0.12 MiB** | 98.06 ± 0.03 MiB |
| generated/wide-object | **22.91 ± 0.05 MiB** | 114.80 ± 0.45 MiB |
| large/api-catalog | **5.30 ± 0.00 MiB** | 34.67 MiB |
| large/circleci | 11.03 ± 0.06 MiB | — |
| massive/data-warehouse | **31.41 ± 0.04 MiB** | 175.94 MiB |
| massive/k8s-deployment | **17.42 MiB** | 31.81 ± 0.02 MiB |
| medium/package-json | 37.59 ± 0.04 MiB | — |
| medium/user-profile | **4.83 MiB** | 33.56 ± 0.01 MiB |
| small/config | **4.33 ± 0.01 MiB** | 32.97 ± 0.02 MiB |
| small/prettierrc | 5.55 ± 0.01 MiB | — |

### validate (valid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.434 ± 0.435 ms | 3.994 ± 0.047 ms | **1.771 ± 0.036 ms** |
| generated/large-array | 57.954 ± 0.865 ms | 44.140 ± 0.606 ms | 43.315 ± 0.285 ms |
| generated/many-defs | 22.910 ± 0.969 ms | 11.120 ± 0.071 ms | **7.200 ± 0.047 ms** |
| generated/wide-object | 27.547 ± 0.107 ms | 7.831 ± 0.078 ms | **3.662 ± 0.036 ms** |
| large/api-catalog | 2.977 ± 0.033 ms | 3.734 ± 0.042 ms | **1.715 ± 0.025 ms** |
| large/circleci | 6.640 ± 0.068 ms | 5.465 ± 0.045 ms | **2.462 ± 0.047 ms** |
| massive/data-warehouse | 49.401 ± 0.248 ms | 11.797 ± 0.166 ms | **5.189 ± 0.051 ms** |
| massive/k8s-deployment | 11.462 ± 0.099 ms | 480.467 ± 145.516 ms † | **2.742 ± 0.044 ms** |
| medium/package-json | 73.910 ± 0.226 ms | — | **7.287 ± 0.069 ms** |
| medium/user-profile | 2.817 ± 0.043 ms | 3.628 ± 0.048 ms | **1.633 ± 0.030 ms** |
| small/config | 2.761 ± 0.027 ms | 3.555 ± 0.142 ms | **1.590 ± 0.043 ms** |
| small/prettierrc | 2.977 ± 0.026 ms | 3.607 ± 0.049 ms | **1.698 ± 0.042 ms** |

### validate (valid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.97 ± 0.03 MiB | 9.89 MiB | **4.06 ± 0.03 MiB** |
| generated/large-array | **55.80 MiB** | 64.69 ± 0.01 MiB | 59.61 ± 0.01 MiB |
| generated/many-defs | 20.70 ± 0.84 MiB | 21.36 MiB | **13.11 ± 0.08 MiB** |
| generated/wide-object | 15.11 MiB | 15.27 ± 0.01 MiB | **7.27 ± 0.01 MiB** |
| large/api-catalog | 5.89 ± 0.01 MiB | 10.34 ± 0.01 MiB | **3.91 ± 0.00 MiB** |
| large/circleci | 9.09 MiB | 13.06 MiB | **5.38 MiB** |
| massive/data-warehouse | 23.19 ± 0.29 MiB | 21.00 MiB | **9.45 ± 0.08 MiB** |
| massive/k8s-deployment | 11.80 MiB | 20.48 ± 0.07 MiB | **5.98 MiB** |
| medium/package-json | 27.67 MiB | — | **11.50 MiB** |
| medium/user-profile | 5.72 MiB | 10.08 MiB | **3.72 MiB** |
| small/config | 5.72 MiB | 9.86 ± 0.01 MiB | **3.62 ± 0.01 MiB** |
| small/prettierrc | 5.83 ± 0.00 MiB | 8.97 ± 0.01 MiB | **3.83 ± 0.01 MiB** |

### validate (invalid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.040 ± 0.039 ms | 3.968 ± 0.046 ms | **1.757 ± 0.039 ms** |
| generated/large-array | 57.627 ± 0.354 ms | 43.812 ± 0.456 ms | 43.369 ± 1.224 ms |
| generated/many-defs | 22.719 ± 0.327 ms | 11.201 ± 0.108 ms | **7.220 ± 0.075 ms** |
| generated/wide-object | 27.509 ± 0.156 ms | 7.962 ± 0.120 ms | **3.678 ± 0.036 ms** |
| large/api-catalog | 2.993 ± 0.034 ms | 3.920 ± 0.223 ms | **1.711 ± 0.031 ms** |
| large/circleci | 6.649 ± 0.058 ms | 5.455 ± 0.056 ms | **2.470 ± 0.034 ms** |
| massive/data-warehouse | 50.564 ± 0.355 ms | 11.835 ± 0.113 ms | **5.338 ± 0.157 ms** |
| massive/k8s-deployment | 11.659 ± 0.476 ms | 404.673 ± 148.021 ms † | **2.767 ± 0.028 ms** |
| medium/package-json | 73.857 ± 0.160 ms | — | **7.277 ± 0.055 ms** |
| medium/user-profile | 2.829 ± 0.030 ms | 3.624 ± 0.027 ms | **1.611 ± 0.033 ms** |
| small/config | 2.779 ± 0.030 ms | 3.605 ± 0.178 ms | **1.599 ± 0.018 ms** |
| small/prettierrc | 2.975 ± 0.046 ms | 3.585 ± 0.051 ms | **1.693 ± 0.029 ms** |

### validate (invalid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.98 MiB | 10.03 ± 0.01 MiB | **4.14 ± 0.02 MiB** |
| generated/large-array | **55.81 ± 0.01 MiB** | 64.80 ± 0.00 MiB | 59.64 MiB |
| generated/many-defs | 19.09 MiB | 21.44 ± 0.03 MiB | **13.03 ± 0.01 MiB** |
| generated/wide-object | 15.12 MiB | 15.42 ± 0.02 MiB | **7.33 ± 0.01 MiB** |
| large/api-catalog | 5.89 ± 0.01 MiB | 10.50 ± 0.01 MiB | **3.97 MiB** |
| large/circleci | 9.08 ± 0.01 MiB | 13.12 ± 0.02 MiB | **5.44 ± 0.01 MiB** |
| massive/data-warehouse | 22.52 ± 0.11 MiB | 21.06 ± 0.01 MiB | **9.59 MiB** |
| massive/k8s-deployment | 11.80 ± 0.01 MiB | 20.47 ± 0.03 MiB | **5.98 ± 0.01 MiB** |
| medium/package-json | 27.67 MiB | — | **11.62 ± 0.01 MiB** |
| medium/user-profile | 5.73 MiB | 10.23 ± 0.02 MiB | **3.75 ± 0.01 MiB** |
| small/config | 5.72 ± 0.02 MiB | 10.02 ± 0.01 MiB | **3.69 MiB** |
| small/prettierrc | 5.84 MiB | 9.12 ± 0.01 MiB | **3.89 ± 0.01 MiB** |

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
