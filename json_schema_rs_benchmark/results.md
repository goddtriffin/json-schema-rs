# Benchmark results

Two tiers, lower is better everywhere. **In-process**: every tool is linked as a Rust library into one process; time is criterion's median ± standard deviation per call, and peak heap is the extra heap the operation allocates, recorded by a counting allocator. **CLI**: every tool's binary runs end to end (process startup, file I/O, and the operation), timed by hyperfine as median ± standard deviation over repeated runs after warmup; peak RSS is the process's peak resident set size. Tools run on different runtimes (see Tools), and process start, runtime boot and module loading are part of every CLI run: each CLI table's *startup* row is the tool on a trivial input (a draft-07 `{"type": "object"}` schema), so that fixed cost can be told apart from the work, and it never counts toward a winner. A cell is measured only when the tool is correct on it (see Compatibility). A cell's best tool wins outright (**bold**) only when at least two tools were measured and its lead over the runner-up exceeds both 3% of its median and the sum of their standard deviations; otherwise the cell is a tie or uncontested and nothing is bolded. `†` marks a measurement whose standard deviation exceeds 10% of its median.

## Tools

| Tool | Version | Language / runtime |
|---|---|---|
| ajv | 8.20.0 | JavaScript (Node.js 26.13.0) |
| blaze | 17.3.2 | C++ (native) |
| boon | 0.6.1 | Rust (native) |
| datamodel-code-generator | 0.88.0 | Python (CPython 3.15.2) |
| fastjsonschema | 2.22.3 | Python (CPython 3.15.2) |
| hyperfine | 1.20.0 | Rust (native); times the CLI tier |
| json-schema-to-typescript | 16.0.0 | TypeScript (Node.js 26.13.0) |
| jsonschema | 0.60.2 | Rust (native) |
| jsonschema-cli | 0.60.2 | Rust (native) |
| jsonschemars | 0.0.6 | Rust (native) |
| python-jsonschema | 4.26.0 | Python (CPython 3.15.2) |
| quicktype | 26.0.0 | TypeScript (Node.js 26.13.0) |
| rustc | 1.99.1 | compiles every Rust tool (release profile) |
| typify | 0.8.0 | Rust (native) |

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
| CLI | codegen | time | jsonschemars | 11 / 11 |
| CLI | codegen | peak RSS | jsonschemars | 11 / 11 |
| CLI | validate (valid) | time | jsonschemars | 11 / 12 |
| CLI | validate (valid) | peak RSS | jsonschemars | 11 / 12 |
| CLI | validate (invalid) | time | jsonschemars | 11 / 12 |
| CLI | validate (invalid) | peak RSS | jsonschemars | 11 / 12 |

## In-process (library calls)

### codegen — time

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | **3.052 ± 0.020 ms** | 9.609 ± 0.013 ms |
| generated/large-array | **134.674 ± 0.511 µs** | 1.025 ± 0.008 ms |
| generated/many-defs | **20.237 ± 0.028 ms** | 121.210 ± 1.715 ms |
| generated/wide-object | **17.824 ± 0.015 ms** | 146.914 ± 0.427 ms |
| large/api-catalog | **461.378 ± 0.548 µs** | 3.072 ± 0.002 ms |
| large/circleci | 6.856 ± 0.005 ms | — |
| massive/data-warehouse | **36.615 ± 0.222 ms** | 268.300 ± 1.887 ms |
| massive/k8s-deployment | 15.619 ± 0.014 ms | **1.794 ± 0.001 ms** |
| medium/package-json | 43.978 ± 0.262 ms | — |
| medium/user-profile | **185.739 ± 0.215 µs** | 1.413 ± 0.021 ms |
| small/config | **64.358 ± 0.074 µs** | 505.108 ± 0.423 µs |
| small/prettierrc | 864.880 ± 17.624 µs | — |

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
| generated/deep-nesting | 1.018 ± 0.002 ms | 204.418 ± 0.216 µs | **99.942 ± 0.141 µs** |
| generated/large-array | 44.982 ± 0.144 µs | 20.553 ± 0.023 µs | **10.591 ± 0.020 µs** |
| generated/many-defs | 13.801 ± 0.031 ms | 2.968 ± 0.004 ms | **1.614 ± 0.002 ms** |
| generated/wide-object | 23.180 ± 0.031 ms | 2.475 ± 0.002 ms | **1.300 ± 0.003 ms** |
| large/api-catalog | 177.247 ± 0.172 µs | 101.838 ± 0.113 µs | **43.353 ± 0.031 µs** |
| large/circleci | 2.846 ± 0.010 ms | 1.008 ± 0.002 ms | **509.958 ± 0.602 µs** |
| massive/data-warehouse | 44.584 ± 0.051 ms | 6.241 ± 0.011 ms | **2.615 ± 0.002 ms** |
| massive/k8s-deployment | 7.325 ± 0.007 ms | 1.580 ± 0.001 ms | **739.135 ± 1.004 µs** |
| medium/package-json | 68.149 ± 0.269 ms | 14.131 ± 0.026 ms | **5.831 ± 0.027 ms** |
| medium/user-profile | 66.385 ± 0.506 µs | 39.420 ± 0.211 µs | **16.375 ± 0.223 µs** |
| small/config | 26.716 ± 0.016 µs | 18.757 ± 0.051 µs | **7.421 ± 0.027 µs** |
| small/prettierrc | 202.498 ± 1.095 µs | 108.925 ± 0.092 µs | **56.641 ± 0.045 µs** |

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
| generated/deep-nesting | 6.550 ± 0.012 µs | **2.432 ± 0.017 µs** | 3.045 ± 0.037 µs |
| generated/large-array | 17.837 ± 0.125 ms | **7.461 ± 0.064 ms** | 8.687 ± 0.134 ms |
| generated/many-defs | 1.722 ± 0.006 ms | **617.484 ± 1.550 µs** | 744.643 ± 13.423 µs |
| generated/wide-object | 169.796 ± 0.268 µs | 113.365 ± 0.618 µs | **54.862 ± 0.158 µs** |
| large/api-catalog | 563.265 ± 2.754 ns | **244.617 ± 1.236 ns** | 415.760 ± 3.771 ns |
| large/circleci | 2.192 ± 0.022 µs | **474.590 ± 5.404 ns** | 1.011 ± 0.015 µs |
| massive/data-warehouse | 69.232 ± 0.444 µs | **23.982 ± 0.185 µs** | 45.465 ± 0.226 µs |
| massive/k8s-deployment | 515.172 ± 2.403 ns | **211.518 ± 1.183 ns** | 491.646 ± 1.440 ns |
| medium/package-json | 537.465 ± 2.228 ns | **229.981 ± 1.828 ns** | 485.761 ± 0.405 ns |
| medium/user-profile | 836.477 ± 1.582 ns | **375.955 ± 1.952 ns** | 488.611 ± 1.833 ns |
| small/config | 223.539 ± 0.789 ns | **112.731 ± 0.297 ns** | 141.834 ± 2.675 ns |
| small/prettierrc | 726.519 ± 3.214 ns | **172.767 ± 1.096 ns** | 275.074 ± 1.596 ns |

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
| generated/deep-nesting | 7.787 ± 0.014 µs | **2.819 ± 0.006 µs** | 5.097 ± 0.015 µs |
| generated/large-array | 17.887 ± 0.075 ms | **7.410 ± 0.032 ms** | 8.699 ± 0.050 ms |
| generated/many-defs | 1.735 ± 0.008 ms | **622.293 ± 1.128 µs** | 764.464 ± 13.916 µs |
| generated/wide-object | 168.874 ± 0.307 µs | 107.248 ± 0.286 µs | **54.067 ± 0.046 µs** |
| large/api-catalog | 653.215 ± 1.691 ns | **393.869 ± 1.700 ns** | 1.022 ± 0.010 µs |
| large/circleci | **127.096 ± 0.311 ns** | 131.030 ± 0.514 ns | 265.302 ± 1.254 ns |
| massive/data-warehouse | 180.276 ± 0.311 µs | **139.908 ± 0.771 µs** | 271.130 ± 1.106 µs |
| massive/k8s-deployment | 175.686 ± 1.692 ns | **134.195 ± 0.168 ns** | 243.148 ± 1.325 ns |
| medium/package-json | 172.404 ± 0.509 ns | **153.167 ± 0.472 ns** | 243.761 ± 0.594 ns |
| medium/user-profile | 367.462 ± 0.936 ns | **314.388 ± 3.218 ns** | 534.607 ± 1.975 ns |
| small/config | 285.595 ± 0.616 ns | **247.105 ± 0.819 ns** | 525.206 ± 2.573 ns |
| small/prettierrc | **531.870 ± 1.550 ns** | 915.076 ± 1.964 ns | 1.492 ± 0.006 µs |

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

| Fixture | datamodel-code-generator | json-schema-to-typescript | jsonschemars | quicktype | typify |
|---|---|---|---|---|---|
| *startup* | 204.926 ± 0.671 ms | 153.858 ± 0.806 ms | 2.250 ± 0.253 ms † | 261.913 ± 2.092 ms | 85.085 ± 0.839 ms |
| generated/deep-nesting | 221.262 ± 1.322 ms | 167.447 ± 0.939 ms | **5.969 ± 0.104 ms** | 286.958 ± 5.042 ms | 109.767 ± 0.414 ms |
| generated/large-array | 214.756 ± 0.619 ms | 159.395 ± 0.898 ms | **2.133 ± 0.029 ms** | 270.906 ± 1.937 ms | 89.666 ± 0.734 ms |
| generated/many-defs | 447.776 ± 1.956 ms | 236.393 ± 1.614 ms | **27.840 ± 0.163 ms** | 439.513 ± 5.459 ms | 437.315 ± 2.774 ms |
| generated/wide-object | 586.246 ± 2.128 ms | 283.146 ± 1.701 ms | **24.512 ± 0.356 ms** | 463.329 ± 4.937 ms | 507.282 ± 3.948 ms |
| large/api-catalog | 221.886 ± 0.755 ms | 162.819 ± 1.109 ms | **2.647 ± 0.024 ms** | 283.966 ± 2.736 ms | 95.729 ± 0.851 ms |
| large/circleci | 285.382 ± 2.009 ms | 199.017 ± 1.090 ms | **10.990 ± 0.137 ms** | — | — |
| massive/data-warehouse | 1.004 ± 0.004 s | 320.019 ± 1.615 ms | **43.242 ± 0.500 ms** | 598.052 ± 2.939 ms | 787.293 ± 4.334 ms |
| massive/k8s-deployment | 413.722 ± 1.604 ms | 262.051 ± 1.890 ms | **21.372 ± 0.273 ms** | 410.045 ± 5.777 ms | 88.111 ± 1.913 ms |
| medium/package-json | — | — | 97.255 ± 0.529 ms | — | — |
| medium/user-profile | 215.566 ± 2.083 ms | 159.101 ± 0.852 ms | **2.189 ± 0.037 ms** | 275.453 ± 1.575 ms | 91.327 ± 1.282 ms |
| small/config | 207.471 ± 1.647 ms | 155.694 ± 1.436 ms | **1.967 ± 0.043 ms** | 267.898 ± 1.298 ms | 87.684 ± 1.228 ms |
| small/prettierrc | 249.474 ± 0.725 ms | 169.521 ± 0.661 ms | **3.247 ± 0.082 ms** | 291.717 ± 2.004 ms | — |

### codegen — peak RSS

| Fixture | datamodel-code-generator | json-schema-to-typescript | jsonschemars | quicktype | typify |
|---|---|---|---|---|---|
| *startup* | 57.77 MiB | 73.38 ± 0.27 MiB | 3.72 ± 0.01 MiB | 127.08 MiB | 31.67 ± 0.03 MiB |
| generated/deep-nesting | 59.94 ± 0.23 MiB | 78.08 MiB | **7.64 ± 0.01 MiB** | 168.39 ± 0.31 MiB | 37.92 MiB |
| generated/large-array | 59.41 ± 0.03 MiB | 75.22 ± 0.02 MiB | **4.62 ± 0.01 MiB** | 130.67 ± 0.37 MiB | 33.16 ± 0.01 MiB |
| generated/many-defs | 81.95 MiB | 123.11 ± 0.40 MiB | **22.41 ± 0.08 MiB** | 218.98 MiB | 98.05 MiB |
| generated/wide-object | 96.22 ± 0.03 MiB | 166.81 ± 0.55 MiB | **22.58 ± 0.03 MiB** | 232.27 ± 0.09 MiB | 114.42 ± 0.33 MiB |
| large/api-catalog | 60.34 MiB | 75.73 ± 0.01 MiB | **5.45 MiB** | 142.78 ± 0.38 MiB | 34.55 MiB |
| large/circleci | 67.06 ± 0.26 MiB | 90.94 ± 0.04 MiB | **11.17 ± 0.01 MiB** | — | — |
| massive/data-warehouse | 173.44 ± 0.33 MiB | 178.02 ± 0.79 MiB | **31.41 ± 0.14 MiB** | 233.12 MiB | 175.80 ± 0.03 MiB |
| massive/k8s-deployment | 84.56 ± 0.38 MiB | 129.34 ± 0.05 MiB | **17.70 MiB** | 215.11 ± 1.09 MiB | 31.64 ± 0.01 MiB |
| medium/package-json | — | — | 41.08 MiB | — | — |
| medium/user-profile | 59.58 ± 0.45 MiB | 75.03 MiB | **4.84 ± 0.02 MiB** | 133.00 ± 0.18 MiB | 33.38 MiB |
| small/config | 58.00 ± 0.01 MiB | 74.52 ± 0.12 MiB | **4.33 ± 0.00 MiB** | 127.56 ± 0.09 MiB | 32.75 ± 0.01 MiB |
| small/prettierrc | 63.61 ± 0.53 MiB | 76.92 ± 0.02 MiB | **5.73 ± 0.03 MiB** | 148.05 ± 0.05 MiB | — |

### validate (valid) — time

| Fixture | ajv | blaze | boon | fastjsonschema | jsonschema-cli | jsonschemars | python-jsonschema |
|---|---|---|---|---|---|---|---|
| *startup* | 112.853 ± 1.015 ms | 3.783 ± 0.341 ms | 3.433 ± 0.333 ms | 24.413 ± 0.649 ms | 3.533 ± 0.265 ms | 1.586 ± 0.047 ms | 59.249 ± 0.328 ms |
| generated/deep-nesting | 135.765 ± 0.908 ms | 14.239 ± 0.315 ms | 4.071 ± 0.081 ms | 94.179 ± 0.400 ms | 4.034 ± 0.219 ms | **1.797 ± 0.035 ms** | 74.234 ± 1.027 ms |
| generated/large-array | 158.223 ± 1.010 ms | 98.184 ± 0.443 ms | 58.697 ± 0.419 ms | 145.987 ± 0.784 ms | 44.999 ± 6.529 ms † | 43.817 ± 0.585 ms | 1.376 ± 0.010 s |
| generated/many-defs | 238.393 ± 1.309 ms | 115.360 ± 0.392 ms | 22.763 ± 0.306 ms | 382.607 ± 0.538 ms | 11.606 ± 0.221 ms | **7.343 ± 0.238 ms** | 413.944 ± 2.482 ms |
| generated/wide-object | — | 58.511 ± 0.436 ms | 27.938 ± 0.197 ms | 303.735 ± 1.161 ms | 8.164 ± 0.116 ms | **3.833 ± 0.047 ms** | 293.091 ± 1.659 ms |
| large/api-catalog | 124.822 ± 1.202 ms | 4.788 ± 0.097 ms | 3.114 ± 0.143 ms | 36.365 ± 0.201 ms | 4.061 ± 0.199 ms | **1.726 ± 0.032 ms** | 65.303 ± 0.308 ms |
| large/circleci | 164.415 ± 1.184 ms | 28.266 ± 0.179 ms | 7.114 ± 0.354 ms | 322.856 ± 0.809 ms | 5.899 ± 0.092 ms | **2.537 ± 0.049 ms** | 73.788 ± 0.460 ms |
| massive/data-warehouse | 552.319 ± 6.100 ms | 238.687 ± 1.508 ms | 49.589 ± 0.245 ms | 1.215 ± 0.002 s | 11.936 ± 0.110 ms | **5.345 ± 0.103 ms** | 457.498 ± 2.124 ms |
| massive/k8s-deployment | — | — | 11.878 ± 0.234 ms | 356.368 ± 0.888 ms | 243.888 ± 31.610 ms † | **2.940 ± 0.052 ms** | 218.197 ± 0.984 ms |
| medium/package-json | 839.973 ± 2.637 ms | 358.821 ± 2.313 ms | 74.336 ± 0.214 ms | 869.358 ± 1.357 ms | — | **7.925 ± 0.322 ms** | 68.312 ± 0.414 ms |
| medium/user-profile | 121.523 ± 0.617 ms | 3.778 ± 0.099 ms | 2.944 ± 0.179 ms | 28.271 ± 0.184 ms | 3.915 ± 0.426 ms † | **1.684 ± 0.031 ms** | 61.436 ± 0.318 ms |
| small/config | 120.061 ± 0.955 ms | 3.243 ± 0.138 ms | 2.854 ± 0.217 ms | 25.380 ± 0.493 ms | 3.832 ± 0.494 ms † | **1.634 ± 0.030 ms** | 59.626 ± 0.423 ms |
| small/prettierrc | 124.350 ± 1.299 ms | 5.917 ± 0.084 ms | 3.132 ± 0.108 ms | 38.056 ± 0.299 ms | 3.944 ± 0.206 ms | **1.776 ± 0.050 ms** | 61.582 ± 1.886 ms |

### validate (valid) — peak RSS

| Fixture | ajv | blaze | boon | fastjsonschema | jsonschema-cli | jsonschemars | python-jsonschema |
|---|---|---|---|---|---|---|---|
| *startup* | 53.22 ± 0.01 MiB | 8.50 MiB | 5.64 MiB | 17.20 ± 0.02 MiB | 8.45 ± 0.01 MiB | 3.47 MiB | 24.69 ± 0.02 MiB |
| generated/deep-nesting | 62.56 ± 0.04 MiB | 18.08 ± 0.01 MiB | 6.92 ± 0.01 MiB | 89.08 MiB | 10.03 ± 0.03 MiB | **4.17 ± 0.00 MiB** | 25.02 ± 0.02 MiB |
| generated/large-array | 84.30 ± 0.60 MiB | 300.80 MiB | **55.84 MiB** | 60.22 ± 0.01 MiB | 64.88 MiB | 59.66 ± 0.01 MiB | 65.66 ± 0.01 MiB |
| generated/many-defs | 80.45 ± 0.03 MiB | 84.83 MiB | 19.16 ± 0.01 MiB | 327.06 ± 0.02 MiB | 21.62 ± 0.03 MiB | **13.19 ± 0.01 MiB** | 28.53 ± 0.14 MiB |
| generated/wide-object | — | 26.86 MiB | 15.22 ± 0.01 MiB | 278.05 ± 0.03 MiB | 15.48 ± 0.01 MiB | **7.34 ± 0.02 MiB** | 26.23 ± 0.01 MiB |
| large/api-catalog | 55.53 MiB | 9.55 MiB | 5.92 MiB | 27.41 ± 0.02 MiB | 10.58 MiB | **3.94 ± 0.01 MiB** | 24.75 ± 0.05 MiB |
| large/circleci | 75.39 ± 0.05 MiB | 19.53 MiB | 9.20 ± 0.08 MiB | 271.59 ± 0.49 MiB | 13.34 MiB | **5.42 MiB** | 25.22 MiB |
| massive/data-warehouse | 153.12 ± 1.51 MiB | 297.67 MiB | 22.02 ± 0.06 MiB | 971.52 ± 0.01 MiB | 21.27 ± 0.02 MiB | **9.53 ± 0.01 MiB** | 27.33 ± 0.00 MiB |
| massive/k8s-deployment | — | — | 11.92 ± 0.02 MiB | 313.31 ± 2.73 MiB | 20.23 MiB | **6.09 ± 0.02 MiB** | 26.31 ± 0.02 MiB |
| medium/package-json | 361.98 ± 0.23 MiB | 68.64 MiB | 28.69 ± 0.08 MiB | 661.61 ± 0.96 MiB | — | **11.77 ± 0.02 MiB** | 28.11 ± 0.02 MiB |
| medium/user-profile | 54.59 ± 0.10 MiB | 9.22 MiB | 5.81 MiB | 20.45 ± 0.01 MiB | 10.36 ± 0.02 MiB | **3.83 ± 0.01 MiB** | 24.64 ± 0.03 MiB |
| small/config | 54.31 MiB | 8.91 MiB | 5.77 ± 0.01 MiB | 18.00 ± 0.01 MiB | 10.16 ± 0.01 MiB | **3.72 MiB** | 24.64 ± 0.05 MiB |
| small/prettierrc | 57.92 ± 0.18 MiB | 10.11 MiB | 5.89 ± 0.02 MiB | 30.06 MiB | 9.22 MiB | **3.89 MiB** | 24.78 MiB |

### validate (invalid) — time

| Fixture | ajv | blaze | boon | fastjsonschema | jsonschema-cli | jsonschemars | python-jsonschema |
|---|---|---|---|---|---|---|---|
| *startup* | 113.150 ± 0.436 ms | 3.064 ± 0.156 ms | 2.801 ± 0.041 ms | 24.544 ± 0.666 ms | 3.519 ± 0.086 ms | 1.596 ± 0.045 ms | 59.107 ± 1.153 ms |
| generated/deep-nesting | 135.843 ± 1.003 ms | 35.352 ± 0.260 ms | 4.058 ± 0.045 ms | 94.521 ± 0.624 ms | 3.963 ± 0.045 ms | **1.776 ± 0.056 ms** | 74.223 ± 0.504 ms |
| generated/large-array | 158.588 ± 0.727 ms | 113.871 ± 0.856 ms | 58.756 ± 0.575 ms | 146.202 ± 0.699 ms | 44.856 ± 0.397 ms | 43.955 ± 0.358 ms | 1.386 ± 0.011 s |
| generated/many-defs | 238.867 ± 0.976 ms | 119.882 ± 0.378 ms | 23.092 ± 0.404 ms | 383.623 ± 1.546 ms | 11.462 ± 0.067 ms | **7.306 ± 0.065 ms** | 416.217 ± 2.172 ms |
| generated/wide-object | — | 58.123 ± 0.773 ms | 27.795 ± 0.196 ms | 304.414 ± 0.835 ms | 8.181 ± 0.052 ms | **3.898 ± 0.144 ms** | 294.765 ± 2.123 ms |
| large/api-catalog | 125.199 ± 0.951 ms | 5.102 ± 0.072 ms | 3.154 ± 0.094 ms | 36.329 ± 0.233 ms | 3.995 ± 0.055 ms | **1.755 ± 0.043 ms** | 65.289 ± 0.324 ms |
| large/circleci | 162.242 ± 1.476 ms | 28.197 ± 0.236 ms | 6.857 ± 0.074 ms | 322.877 ± 1.411 ms | 5.805 ± 0.050 ms | **2.656 ± 0.157 ms** | 73.629 ± 0.682 ms |
| massive/data-warehouse | 491.991 ± 1.860 ms | 303.392 ± 0.930 ms | 50.970 ± 0.373 ms | 1.214 ± 0.001 s | 11.905 ± 0.189 ms | **5.488 ± 0.101 ms** | 459.743 ± 1.738 ms |
| massive/k8s-deployment | — | — | 11.819 ± 0.086 ms | 358.974 ± 13.787 ms | 246.523 ± 29.014 ms † | **2.974 ± 0.165 ms** | 220.040 ± 1.396 ms |
| medium/package-json | 839.308 ± 3.447 ms | 359.242 ± 1.231 ms | 74.441 ± 0.357 ms | 868.380 ± 2.448 ms | — | **7.587 ± 0.058 ms** | 68.725 ± 0.331 ms |
| medium/user-profile | 122.156 ± 0.559 ms | 3.865 ± 0.077 ms | 3.052 ± 0.210 ms | 28.241 ± 0.881 ms | 3.861 ± 0.068 ms | **1.649 ± 0.089 ms** | 61.510 ± 0.423 ms |
| small/config | 120.029 ± 0.629 ms | 3.371 ± 0.131 ms | 3.008 ± 0.214 ms | 25.434 ± 0.419 ms | 3.812 ± 0.071 ms | **1.640 ± 0.073 ms** | 59.682 ± 0.362 ms |
| small/prettierrc | 123.619 ± 1.309 ms | 6.262 ± 0.177 ms | 3.189 ± 0.098 ms | 38.196 ± 0.275 ms | 3.901 ± 0.049 ms | **1.769 ± 0.060 ms** | 60.972 ± 0.419 ms |

### validate (invalid) — peak RSS

| Fixture | ajv | blaze | boon | fastjsonschema | jsonschema-cli | jsonschemars | python-jsonschema |
|---|---|---|---|---|---|---|---|
| *startup* | 53.31 ± 0.04 MiB | 8.67 MiB | 5.64 MiB | 17.19 ± 0.01 MiB | 8.56 ± 0.01 MiB | 3.52 MiB | 24.77 MiB |
| generated/deep-nesting | 62.50 MiB | 18.27 MiB | 6.94 ± 0.02 MiB | 88.66 MiB | 10.16 MiB | **4.28 ± 0.03 MiB** | 25.03 ± 0.02 MiB |
| generated/large-array | 84.22 ± 0.04 MiB | 326.53 MiB | **55.88 MiB** | 60.27 ± 0.03 MiB | 65.00 MiB | 59.69 ± 0.01 MiB | 65.64 ± 0.02 MiB |
| generated/many-defs | 80.59 ± 0.07 MiB | 85.44 MiB | 19.17 ± 0.01 MiB | 327.06 MiB | 21.69 MiB | **13.27 MiB** | 28.23 ± 0.01 MiB |
| generated/wide-object | — | 27.56 MiB | 15.20 ± 0.01 MiB | 277.98 ± 2.64 MiB | 15.66 MiB | **7.45 ± 0.01 MiB** | 26.27 MiB |
| large/api-catalog | 55.66 ± 0.08 MiB | 9.69 MiB | 5.95 ± 0.01 MiB | 27.45 ± 0.01 MiB | 10.70 MiB | **3.98 MiB** | 24.70 MiB |
| large/circleci | 75.75 ± 0.29 MiB | 19.47 MiB | 9.28 MiB | 270.19 MiB | 13.31 MiB | **5.50 ± 0.01 MiB** | 25.16 ± 0.04 MiB |
| massive/data-warehouse | 140.72 MiB | 68.66 MiB | 22.72 ± 0.23 MiB | 971.72 ± 0.07 MiB | 21.27 ± 0.02 MiB | **9.70 MiB** | 28.09 ± 0.04 MiB |
| massive/k8s-deployment | — | — | 11.88 ± 0.02 MiB | 316.14 ± 0.08 MiB | 20.27 MiB | **6.09 MiB** | 26.39 ± 0.02 MiB |
| medium/package-json | 362.09 ± 0.51 MiB | 68.72 MiB | 27.89 MiB | 661.67 ± 1.71 MiB | — | **11.80 MiB** | 28.17 ± 0.02 MiB |
| medium/user-profile | 54.75 ± 0.03 MiB | 9.31 MiB | 5.88 MiB | 20.44 ± 0.01 MiB | 10.47 ± 0.02 MiB | **3.84 ± 0.00 MiB** | 24.77 ± 0.06 MiB |
| small/config | 54.38 ± 0.03 MiB | 9.06 MiB | 5.80 ± 0.01 MiB | 18.08 MiB | 10.31 ± 0.00 MiB | **3.78 ± 0.01 MiB** | 24.70 ± 0.05 MiB |
| small/prettierrc | 58.06 ± 0.05 MiB | 10.19 ± 0.01 MiB | 5.92 ± 0.01 MiB | 30.08 MiB | 9.36 ± 0.01 MiB | **3.94 ± 0.01 MiB** | 24.78 ± 0.06 MiB |

## Compatibility

361 of 379 attempted cells pass. Rows with at least one failure:

| Tier | Operation | Fixture | ajv | blaze | boon | datamodel-code-generator | fastjsonschema | json-schema-to-typescript | jsonschema | jsonschema-cli | jsonschemars | python-jsonschema | quicktype | typify |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| in-process | codegen | large/circleci | — | — | — | — | — | — | — | — | pass | — | — | FAIL: panic: not implemented: if/then/else schemas are not supported: { "if": { "required": [ "type" ], "properties": { "type": { "const": "release" } } }, "then": { "required": [ "plan_name" ], "properties": { "plan_name": { "type": "string", "markdownDescription": "Required plan name for release jobs." } }, "additionalProperties": true, "markdownDescription": "https://circleci.com/docs/reference/configuration-reference/#job-type\n\nRelease job." }, "else": { "if": { "required": [ "type" ], "properties": { "type": { "enum": [ "lock", "unlock" ] } } }, "then": { "required": [ "key" ], "properties": { "key": { "type": "string", "markdownDescription": "Required key for lock/unlock jobs." } }, "additionalProperties": true, "markdownDescription": "https://circleci.com/docs/reference/configuration-reference/#job-type\n\nA lock/unlock job" }, "else": { "if": { "required": [ "type" ], "properties": { "type": { "enum": [ "approval", "no-op" ] } } }, "then": { "additionalProperties": true, "markdownDescription": "https://circleci.com/docs/reference/configuration-reference/#job-type\n\nApproval/no-op jobs primarily act as workflow markers. Other keys like `steps:` are permitted but will be ignored in the final pipeline." }, "else": { "if": { "anyOf": [ { "not": { "required": [ "type" ] } }, { "required": [ "type" ], "properties": { "type": { "const": "build" } } } ], "markdownDescription": "https://circleci.com/docs/reference/configuration-reference/#job-type\n\nA build job. The job explicitly has type 'build' or defaults to build when no type specified." }, "then": { "anyOf": [ { "type": "object", "required": [ "executor" ], "markdownDescription": "A job that uses an executor defined elsewhere in the configuration." }, { "oneOf": [ { "type": "object", "required": [ "machine" ] }, { "type": "object", "required": [ "docker" ] }, { "type": "object", "required": [ "macos" ] } ] } ], "required": [ "steps" ], "properties": { "circleci_ip_ranges": { "type": "boolean", "markdownDescription": "Enable running jobs with a set of well-defined IP address ranges." }, "description": { "type": "string" }, "docker": { "type": "array", "items": { "type": "object", "required": [ "image" ], "properties": { "auth": { "type": "object", "required": [ "password", "username" ], "properties": { "password": { "type": "string" }, "username": { "type": "string" } }, "additionalProperties": false }, "aws_auth": { "oneOf": [ { "type": "object", "required": [ "aws_access_key_id", "aws_secret_access_key" ], "properties": { "aws_access_key_id": { "type": "string" }, "aws_secret_access_key": { "type": "string" } }, "additionalProperties": false }, { "type": "object", "required": [ "oidc_role_arn" ], "properties": { "oidc_role_arn": { "type": "string" } }, "additionalProperties": false } ] }, "command": { "oneOf": [ { "type": "string" }, { "type": "array", "items": { "type": "string" } } ] }, "entrypoint": { "oneOf": [ { "type": "string" }, { "type": "array", "items": { "type": "string" } } ] }, "environment": { "oneOf": [ { "type": [ "string", "null" ], "markdownDescription": "A map of environment variable names and values." }, { "type": "array", "items": { "$ref": "#/definitions/environment" }, "markdownDescription": "An array of strings in the form KEY=VALUE" }, { "type": "object", "additionalProperties": { "type": [ "string", "boolean", "number", "null" ] }, "markdownDescription": "The value should be a string, but we allow null, boolean and numbers too. Examples: `FOO: true` and `BAR: 2` `BAZ:`" } ] }, "image": { "type": "string" }, "name": { "type": "string" }, "user": { "type": "string" } }, "additionalProperties": false }, "minItems": 1 }, "environment": { "oneOf": [ { "type": [ "string", "null" ], "markdownDescription": "A map of environment variable names and values." }, { "type": "array", "items": { "$ref": "#/definitions/environment" }, "markdownDescription": "An array of strings in the form KEY=VALUE" }, { "type": "object", "additionalProperties": { "type": [ "string", "boolean", "number", "null" ] }, "markdownDescription": "The value should be a string, but we allow null, boolean and numbers too. Examples: `FOO: true` and `BAR: 2` `BAZ:`" } ] }, "executor": { "oneOf": [ { "type": "string", "markdownDescription": "https://circleci.com/docs/reference/reusing-config/#executor\n\nThe name of the executor to use (defined via the top level executors map or from an orb)." }, { "type": "object", "minProperties": 1, "required": [ "name" ], "properties": { "name": { "type": "string" } }, "additionalProperties": true, "markdownDescription": "Executor stanza to use for the job, with optional parameter overrides." } ] }, "machine": { "oneOf": [ { "anyOf": [ { "type": "string", "pattern": " *<< *parameters.([^ ]+) *>> *" }, { "type": "boolean" }, { "type": "number" }, { "type": "string" } ] }, { "type": "object", "properties": { "docker_layer_caching": { "anyOf": [ { "type": "string", "pattern": " *<< *parameters.([^ ]+) *>> *" }, { "type": "boolean" }, { "type": "number" }, { "type": "string" } ], "markdownDescription": "Set to `true` to enable [Docker Layer Caching](https://circleci.com/docs/docker-layer-caching). Note: If you haven't already, you must open a support ticket to have a CircleCI Sales representative contact you about enabling this feature on your account for an additional fee." }, "enabled": { "anyOf": [ { "type": "string", "pattern": " *<< *parameters.([^ ]+) *>> *" }, { "type": "boolean" }, { "type": "number" }, { "type": "string" } ], "markdownDescription": "A boolean or a template parameter evaluating to a boolean" }, "image": { "type": "string" }, "resource_class": { "type": "string" }, "shell": { "type": "string" } }, "additionalProperties": false } ], "markdownDescription": "Use the default machine executor image" }, "macos": { "type": "object", "required": [ "xcode" ], "properties": { "resource_class": { "type": "string" }, "shell": { "type": "string" }, "xcode": { "type": [ "string", "number" ] } }, "additionalProperties": false }, "parallelism": { "anyOf": [ { "oneOf": [ { "type": "string", "pattern": " *<< *parameters.([^ ]+) *>> *" }, { "type": "string", "pattern": " *<< *pipeline.parameters.([^ ]+) *>> *" } ] }, { "type": "integer" } ], "markdownDescription": "A integer or a parameter evaluating to a integer" }, "parameters": { "type": "object", "minProperties": 1, "additionalProperties": { "type": "object", "required": [ "type" ], "properties": { "default": { "oneOf": [ { "type": "string" }, { "type": "boolean" }, { "type": "integer" }, { "type": "array", "items": { "$ref": "#/definitions/step" } } ] }, "description": { "type": "string", "markdownDescription": "A description of the parameter" }, "enum": { "type": "array", "items": { "type": "string" }, "minItems": 1, "markdownDescription": "List of allowed values for an `enum` type parameter" }, "type": { "type": "string", "enum": [ "boolean", "string", "steps", "enum", "executor", "integer", "env_var_name" ], "markdownDescription": "The parameter type. See [Parameter Types](https://circleci.com/docs/reusing-config#parameter-syntax) for details:\n- `boolean`: A boolean parameter\n- `string`: A string parameter \n- `integer`: An integer parameter\n- `enum`: An enum parameter (list of allowed string values)\n- `executor`: An executor parameter (allows job to choose executor)\n- `steps`: A steps parameter (allows mixing predefined and user-defined steps)\n- `env_var_name`: An environment variable name parameter (must match POSIX_NAME regexp)" } }, "additionalProperties": false }, "markdownDescription": "https://circleci.com/docs/reusing-config#using-the-parameters-declaration\n\nA map of parameter keys." }, "resource_class": { "type": "string" }, "retention": { "type": "object", "properties": { "caches": { "type": "string", "pattern": "^([1-9]\|1[0-5])d$" } }, "additionalProperties": false }, "shell": { "oneOf": [ { "type": "string" }, { "type": "array", "items": { "type": "string" } } ] }, "steps": { "type": "array", "items": { "$ref": "#/definitions/step" }, "minItems": 1 }, "type": { "const": "build", "markdownDescription": "The job type. If not specified, defaults to build." }, "working_directory": { "type": "string", "markdownDescription": "In which directory to run the steps. (default: `~/project`. `project` is a literal string, not the name of the project.) You can also refer the directory with `$CIRCLE_WORKING_DIRECTORY` environment variable." } }, "additionalProperties": false, "markdownDescription": "Validation: The job must adhere to the structure of a build job." }, "else": false } } } } |
| in-process | codegen | medium/package-json | — | — | — | — | — | — | — | — | pass | — | — | FAIL: cannot load the fixture's resource documents |
| in-process | codegen | small/prettierrc | — | — | — | — | — | — | — | — | pass | — | — | FAIL: codegen error: value does not conform to the given schema |
| CLI | codegen | large/circleci | — | — | — | pass | — | pass | — | — | pass | — | FAIL: exit 1: Error: Internal error: We have additional properties but want to make a class. | FAIL: exit 101: not implemented: if/then/else schemas are not supported: { |
| CLI | codegen | medium/package-json | — | — | — | FAIL: cannot load the fixture's resource documents | — | FAIL: cannot load the fixture's resource documents | — | — | pass | — | FAIL: cannot load the fixture's resource documents | FAIL: cannot load the fixture's resource documents |
| CLI | codegen | small/prettierrc | — | — | — | pass | — | pass | — | — | pass | — | pass | FAIL: exit 1: value does not conform to the given schema |
| CLI | validate (valid) | generated/wide-object | FAIL: rejected valid instance (exit 2): ajv_validate: error: Maximum call stack size exceeded | pass | pass | — | pass | — | — | pass | pass | pass | — | — |
| CLI | validate (valid) | massive/k8s-deployment | FAIL: rejected valid instance (exit 2): ajv_validate: unsupported $schema http://json-schema.org/schema# | FAIL: rejected valid instance (exit 4): error: Could not resolve the metaschema of the schema | pass | — | pass | — | — | pass | pass | pass | — | — |
| CLI | validate (valid) | medium/package-json | pass | pass | pass | — | pass | — | — | FAIL: cannot load the fixture's resource documents | pass | pass | — | — |
| CLI | validate (invalid) | generated/wide-object | FAIL: not measured: valid instance was rejected | pass | pass | — | pass | — | — | pass | pass | pass | — | — |
| CLI | validate (invalid) | massive/k8s-deployment | FAIL: not measured: valid instance was rejected | FAIL: not measured: valid instance was rejected | pass | — | pass | — | — | pass | pass | pass | — | — |
| CLI | validate (invalid) | medium/package-json | pass | pass | pass | — | pass | — | — | FAIL: cannot load the fixture's resource documents | pass | pass | — | — |
