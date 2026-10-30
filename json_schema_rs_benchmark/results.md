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
| in-process | validate (invalid) | time | jsonschema | 8 / 12 |
| in-process | validate (invalid) | peak heap | jsonschema | 8 / 12 |
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
| generated/deep-nesting | **379.784 ± 6.103 µs** | 9.802 ± 0.324 ms |
| generated/large-array | **44.814 ± 0.359 µs** | 1.038 ± 0.008 ms |
| generated/many-defs | **4.381 ± 0.068 ms** | 123.087 ± 2.517 ms |
| generated/wide-object | **5.706 ± 0.138 ms** | 148.406 ± 1.587 ms |
| large/api-catalog | **163.700 ± 0.789 µs** | 3.103 ± 0.068 ms |
| large/circleci | 1.669 ± 0.085 ms | — |
| massive/data-warehouse | **8.535 ± 0.174 ms** | 271.315 ± 4.757 ms |
| massive/k8s-deployment | 4.829 ± 0.077 ms | **1.804 ± 0.012 ms** |
| medium/package-json | 16.287 ± 0.090 ms | — |
| medium/user-profile | **61.342 ± 0.687 µs** | 1.479 ± 0.053 ms |
| small/config | **20.824 ± 0.372 µs** | 521.987 ± 10.079 µs |
| small/prettierrc | 323.837 ± 3.485 µs | — |

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
| generated/deep-nesting | 1.034 ± 0.018 ms | 208.881 ± 3.436 µs | **101.985 ± 1.626 µs** |
| generated/large-array | 45.161 ± 0.533 µs | 21.095 ± 0.374 µs | **10.718 ± 0.109 µs** |
| generated/many-defs | 14.398 ± 0.435 ms | 3.106 ± 0.063 ms | **1.778 ± 0.055 ms** |
| generated/wide-object | 24.081 ± 0.294 ms | 2.587 ± 0.064 ms | **1.331 ± 0.038 ms** |
| large/api-catalog | 181.857 ± 2.552 µs | 103.774 ± 1.389 µs | **42.918 ± 0.749 µs** |
| large/circleci | 2.991 ± 0.080 ms | 1.054 ± 0.033 ms | **555.299 ± 20.882 µs** |
| massive/data-warehouse | 47.310 ± 1.612 ms | 6.589 ± 0.336 ms | **2.581 ± 0.058 ms** |
| massive/k8s-deployment | 7.706 ± 0.174 ms | 1.684 ± 0.041 ms | **752.662 ± 14.275 µs** |
| medium/package-json | 70.611 ± 1.737 ms | 14.597 ± 0.954 ms | **5.903 ± 0.264 ms** |
| medium/user-profile | 68.746 ± 2.268 µs | 40.401 ± 0.559 µs | **16.504 ± 0.221 µs** |
| small/config | 27.665 ± 0.341 µs | 19.776 ± 0.217 µs | **7.634 ± 0.103 µs** |
| small/prettierrc | 211.935 ± 3.677 µs | 112.114 ± 1.250 µs | **56.550 ± 0.494 µs** |

### schema compile — peak heap

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 636.04 KiB | 534.41 KiB | **152.81 KiB** |
| generated/large-array | 66.67 KiB | 34.10 KiB | **20.41 KiB** |
| generated/many-defs | 8.10 MiB | 3.88 MiB | **2.64 MiB** |
| generated/wide-object | 13.73 MiB | 4.03 MiB | **2.69 MiB** |
| large/api-catalog | 266.71 KiB | 151.24 KiB | **67.54 KiB** |
| large/circleci | 3.85 MiB | 1.75 MiB | **869.03 KiB** |
| massive/data-warehouse | 16.73 MiB | 9.42 MiB | **4.01 MiB** |
| massive/k8s-deployment | 7.45 MiB | 3.28 MiB | **1.45 MiB** |
| medium/package-json | 29.52 MiB | 21.86 MiB | **11.36 MiB** |
| medium/user-profile | 121.82 KiB | 59.13 KiB | **28.40 KiB** |
| small/config | 38.35 KiB | 24.08 KiB | **10.83 KiB** |
| small/prettierrc | 476.85 KiB | 261.04 KiB | **115.30 KiB** |

### validate (valid) — time

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.669 ± 0.042 µs | **2.543 ± 0.059 µs** | 3.211 ± 0.124 µs |
| generated/large-array | 18.260 ± 0.653 ms | **8.027 ± 0.356 ms** | 9.075 ± 0.187 ms |
| generated/many-defs | 1.753 ± 0.012 ms | **622.564 ± 7.654 µs** | 756.294 ± 11.002 µs |
| generated/wide-object | 170.614 ± 0.842 µs | 110.464 ± 4.125 µs | **55.117 ± 0.495 µs** |
| large/api-catalog | 563.658 ± 2.284 ns | **240.135 ± 1.880 ns** | 452.514 ± 13.456 ns |
| large/circleci | 2.225 ± 0.027 µs | **469.362 ± 4.043 ns** | 1.003 ± 0.011 µs |
| massive/data-warehouse | 72.095 ± 0.537 µs | **24.124 ± 0.235 µs** | 47.995 ± 0.431 µs |
| massive/k8s-deployment | 517.475 ± 1.000 ns | **208.652 ± 0.922 ns** | 488.079 ± 3.293 ns |
| medium/package-json | 533.873 ± 1.523 ns | **231.194 ± 0.866 ns** | 481.557 ± 0.371 ns |
| medium/user-profile | 843.262 ± 1.441 ns | **370.449 ± 1.555 ns** | 511.758 ± 3.856 ns |
| small/config | 224.490 ± 0.464 ns | **111.834 ± 0.192 ns** | 138.398 ± 2.096 ns |
| small/prettierrc | 732.964 ± 1.238 ns | **178.270 ± 0.959 ns** | 276.840 ± 3.086 ns |

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
| generated/deep-nesting | 7.446 ± 0.012 µs | **2.787 ± 0.004 µs** | 4.205 ± 0.021 µs |
| generated/large-array | 18.056 ± 0.076 ms | **7.207 ± 0.009 ms** | 8.963 ± 0.105 ms |
| generated/many-defs | 1.750 ± 0.004 ms | **618.436 ± 1.227 µs** | 772.591 ± 1.599 µs |
| generated/wide-object | 169.212 ± 1.078 µs | 115.818 ± 1.176 µs | **55.172 ± 0.081 µs** |
| large/api-catalog | 655.373 ± 3.593 ns | **388.939 ± 1.407 ns** | 679.815 ± 2.291 ns |
| large/circleci | **126.766 ± 1.167 ns** | 131.591 ± 0.422 ns | 205.717 ± 0.439 ns |
| massive/data-warehouse | 179.285 ± 0.777 µs | 139.086 ± 0.404 µs | **55.287 ± 0.325 µs** |
| massive/k8s-deployment | 180.086 ± 0.613 ns | **134.935 ± 0.789 ns** | 175.725 ± 0.371 ns |
| medium/package-json | 170.910 ± 0.281 ns | **149.695 ± 0.746 ns** | 159.856 ± 0.162 ns |
| medium/user-profile | 366.358 ± 0.445 ns | **306.071 ± 0.438 ns** | 346.128 ± 1.299 ns |
| small/config | 294.606 ± 0.528 ns | **244.130 ± 0.361 ns** | 384.061 ± 1.614 ns |
| small/prettierrc | 546.372 ± 0.905 ns | 895.410 ± 3.980 ns | **105.423 ± 0.238 ns** |

### validate (invalid) — peak heap

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 2.97 KiB | **0.75 KiB** | 2.02 KiB |
| generated/large-array | 1.37 KiB | **0.46 KiB** | 0.52 KiB |
| generated/many-defs | 7.77 KiB | **0.77 KiB** | 0.82 KiB |
| generated/wide-object | 0.74 KiB | **0.41 KiB** | 0.45 KiB |
| large/api-catalog | 1.44 KiB | **0.83 KiB** | 1.64 KiB |
| large/circleci | 0.74 KiB | **0.40 KiB** | 0.87 KiB |
| massive/data-warehouse | 467.89 KiB | 111.78 KiB | **0.47 KiB** |
| massive/k8s-deployment | 0.81 KiB | 0.48 KiB | 0.47 KiB |
| medium/package-json | 0.74 KiB | **0.40 KiB** | 0.44 KiB |
| medium/user-profile | 1.30 KiB | 0.73 KiB | **0.63 KiB** |
| small/config | 1.30 KiB | **0.71 KiB** | 1.35 KiB |
| small/prettierrc | 2.38 KiB | 2.69 KiB | **0.41 KiB** |

## CLI (end to end)

### codegen — time

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | **3.089 ± 0.575 ms †** | 108.126 ± 0.606 ms |
| generated/large-array | **1.877 ± 0.063 ms** | 86.919 ± 0.424 ms |
| generated/many-defs | **7.621 ± 0.127 ms** | 435.339 ± 1.506 ms |
| generated/wide-object | **8.892 ± 0.079 ms** | 504.739 ± 2.734 ms |
| large/api-catalog | **2.115 ± 0.061 ms** | 92.866 ± 0.499 ms |
| large/circleci | 4.322 ± 0.039 ms | — |
| massive/data-warehouse | **12.395 ± 0.135 ms** | 784.269 ± 3.258 ms |
| massive/k8s-deployment | **8.046 ± 0.047 ms** | 84.781 ± 0.322 ms |
| medium/package-json | 34.076 ± 0.190 ms | — |
| medium/user-profile | **1.852 ± 0.038 ms** | 87.385 ± 0.764 ms |
| small/config | **1.799 ± 0.059 ms** | 84.189 ± 0.686 ms |
| small/prettierrc | 2.326 ± 0.037 ms | — |

### codegen — peak RSS

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | **5.05 ± 0.01 MiB** | 38.02 ± 0.02 MiB |
| generated/large-array | **3.69 ± 0.00 MiB** | 33.28 ± 0.01 MiB |
| generated/many-defs | **8.72 ± 0.01 MiB** | 97.98 ± 0.02 MiB |
| generated/wide-object | **11.11 ± 0.02 MiB** | 114.59 ± 0.04 MiB |
| large/api-catalog | **4.25 ± 0.01 MiB** | 34.70 MiB |
| large/circleci | 6.62 ± 0.04 MiB | — |
| massive/data-warehouse | **11.59 MiB** | 176.02 ± 0.07 MiB |
| massive/k8s-deployment | **8.77 ± 0.02 MiB** | 31.78 ± 0.01 MiB |
| medium/package-json | 19.73 ± 0.02 MiB | — |
| medium/user-profile | **3.80 MiB** | 33.58 ± 0.01 MiB |
| small/config | **3.59 MiB** | 32.97 ± 0.04 MiB |
| small/prettierrc | 4.69 MiB | — |

### validate (valid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.413 ± 0.435 ms | 4.137 ± 0.070 ms | **1.793 ± 0.033 ms** |
| generated/large-array | 59.268 ± 1.196 ms | 45.030 ± 0.406 ms | 45.500 ± 0.472 ms |
| generated/many-defs | 22.914 ± 0.181 ms | 11.345 ± 0.088 ms | **6.947 ± 0.045 ms** |
| generated/wide-object | 27.636 ± 0.199 ms | 7.904 ± 0.111 ms | **3.835 ± 0.106 ms** |
| large/api-catalog | 3.082 ± 0.037 ms | 3.795 ± 0.031 ms | **1.660 ± 0.038 ms** |
| large/circleci | 6.773 ± 0.061 ms | 5.534 ± 0.058 ms | **2.485 ± 0.051 ms** |
| massive/data-warehouse | 49.348 ± 0.517 ms | 11.841 ± 0.092 ms | **5.081 ± 0.036 ms** |
| massive/k8s-deployment | 11.719 ± 0.078 ms | 353.089 ± 50.202 ms † | **2.778 ± 0.037 ms** |
| medium/package-json | 74.112 ± 0.228 ms | — | — |
| medium/user-profile | 2.842 ± 0.038 ms | 3.603 ± 0.075 ms | **1.593 ± 0.035 ms** |
| small/config | 2.777 ± 0.044 ms | 3.493 ± 0.062 ms | **1.570 ± 0.053 ms** |
| small/prettierrc | 3.051 ± 0.046 ms | 3.579 ± 0.041 ms | **1.639 ± 0.032 ms** |

### validate (valid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.97 MiB | 9.89 ± 0.01 MiB | **4.00 ± 0.01 MiB** |
| generated/large-array | **55.78 ± 0.02 MiB** | 64.70 MiB | 59.47 MiB |
| generated/many-defs | 19.05 ± 0.01 MiB | 21.38 MiB | **12.89 MiB** |
| generated/wide-object | 15.09 ± 0.01 MiB | 15.27 ± 0.01 MiB | **7.17 MiB** |
| large/api-catalog | 5.88 MiB | 10.33 MiB | **3.75 MiB** |
| large/circleci | 9.08 ± 0.02 MiB | 13.06 ± 0.01 MiB | **5.23 MiB** |
| massive/data-warehouse | 22.03 ± 0.01 MiB | 20.97 ± 0.02 MiB | **9.19 MiB** |
| massive/k8s-deployment | 11.75 ± 0.03 MiB | 20.55 ± 0.06 MiB | **5.81 MiB** |
| medium/package-json | 27.67 ± 0.01 MiB | — | — |
| medium/user-profile | 5.72 ± 0.01 MiB | 10.08 ± 0.01 MiB | **3.61 MiB** |
| small/config | 5.72 MiB | 9.86 MiB | **3.56 MiB** |
| small/prettierrc | 5.84 ± 0.00 MiB | 8.97 ± 0.02 MiB | **3.73 ± 0.01 MiB** |

### validate (invalid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.160 ± 0.053 ms | 3.999 ± 0.081 ms | **1.798 ± 0.041 ms** |
| generated/large-array | 59.081 ± 0.328 ms | 44.788 ± 6.728 ms † | 47.486 ± 4.188 ms |
| generated/many-defs | 22.982 ± 0.260 ms | 11.307 ± 0.229 ms | **6.980 ± 0.055 ms** |
| generated/wide-object | 27.638 ± 0.175 ms | 7.848 ± 0.091 ms | **3.742 ± 0.057 ms** |
| large/api-catalog | 3.119 ± 0.041 ms | 3.769 ± 0.044 ms | **1.687 ± 0.076 ms** |
| large/circleci | 6.767 ± 0.045 ms | 5.622 ± 0.062 ms | **2.462 ± 0.041 ms** |
| massive/data-warehouse | 50.398 ± 0.556 ms | 11.859 ± 0.095 ms | **4.983 ± 0.029 ms** |
| massive/k8s-deployment | 11.736 ± 0.077 ms | 343.605 ± 122.171 ms † | **2.751 ± 0.034 ms** |
| medium/package-json | 73.903 ± 0.473 ms | — | — |
| medium/user-profile | 2.902 ± 0.039 ms | 3.630 ± 0.081 ms | **1.603 ± 0.042 ms** |
| small/config | 2.828 ± 0.067 ms | 3.526 ± 0.063 ms | **1.551 ± 0.029 ms** |
| small/prettierrc | 3.076 ± 0.049 ms | 3.767 ± 0.076 ms | **1.676 ± 0.054 ms** |

### validate (invalid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.97 ± 0.01 MiB | 10.02 ± 0.01 MiB | **4.06 MiB** |
| generated/large-array | **55.81 ± 0.02 MiB** | 64.80 ± 0.01 MiB | 59.55 ± 0.02 MiB |
| generated/many-defs | 19.11 ± 0.02 MiB | 21.44 ± 0.01 MiB | **12.95 ± 0.01 MiB** |
| generated/wide-object | 15.09 ± 0.00 MiB | 15.41 MiB | **7.25 MiB** |
| large/api-catalog | 5.91 ± 0.01 MiB | 10.50 MiB | **3.80 ± 0.02 MiB** |
| large/circleci | 9.08 MiB | 13.05 ± 0.02 MiB | **5.30 ± 0.01 MiB** |
| massive/data-warehouse | 22.53 ± 0.08 MiB | 21.05 ± 0.02 MiB | **9.06 ± 0.06 MiB** |
| massive/k8s-deployment | 11.78 ± 0.02 MiB | 20.53 ± 0.04 MiB | **5.91 MiB** |
| medium/package-json | 27.67 MiB | — | — |
| medium/user-profile | 5.73 ± 0.02 MiB | 10.23 MiB | **3.62 MiB** |
| small/config | 5.73 ± 0.02 MiB | 10.03 ± 0.01 MiB | **3.64 MiB** |
| small/prettierrc | 5.86 ± 0.01 MiB | 9.14 ± 0.01 MiB | **3.75 ± 0.01 MiB** |

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
