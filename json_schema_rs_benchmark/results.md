# Benchmark results

Two tiers, lower is better everywhere. **In-process**: every tool is linked as a Rust library and called on in-memory inputs (corvus-json-schema and jsonschema-macros each from a build of its own, so no tool's dependency features reach another's); time is criterion's median ± standard deviation per call, and peak heap is the extra heap the operation allocates, recorded by a counting allocator. jsonschema-macros compiles each schema into a validator when the benchmark is built, so it has no schema compile cells. **CLI**: every tool's binary runs end to end (process startup, file I/O, and the operation), timed by hyperfine as median ± standard deviation over repeated runs after warmup; peak RSS is the process's peak resident set size. Tools run on different runtimes (see Tools), and process start, runtime boot and module loading are part of every CLI run: each CLI table's *startup* row is the tool on a trivial input (a draft-07 `{"type": "object"}` schema), so that fixed cost can be told apart from the work, and it never counts toward a winner. A cell is measured only when the tool is correct on it (see Compatibility). A cell's best tool wins outright (**bold**) only when at least two tools were measured and its lead over the runner-up exceeds both 3% of its median and the sum of their standard deviations; otherwise the cell is a tie or uncontested and nothing is bolded. `†` marks a measurement whose standard deviation exceeds 10% of its median.

## Tools

| Tool | Version | Language / runtime |
|---|---|---|
| ajv | 8.20.0 | JavaScript (Node.js 26.13.0) |
| blaze | 17.3.2 | C++ (native) |
| boon | 0.6.1 | Rust (native) |
| corvus-json-schema | 0.2.1 | Rust (native) |
| datamodel-code-generator | 0.89.0 | Python (CPython 3.15.2) |
| fastjsonschema | 2.22.3 | Python (CPython 3.15.2) |
| hyperfine | 1.20.0 | Rust (native); times the CLI tier |
| json-schema-to-typescript | 16.0.0 | TypeScript (Node.js 26.13.0) |
| jsonschema | 0.60.2 | Rust (native) |
| jsonschema-cli | 0.60.2 | Rust (native) |
| jsonschema-macros | 0.60.2 | Rust (native); validators generated at compile time |
| jsonschema-valid | 0.5.2 | Rust (native) |
| jsonschema_code_generator | 2.0.0 | Rust (native) |
| jsonschemars | 0.0.6 | Rust (native) |
| python-jsonschema | 4.26.0 | Python (CPython 3.15.2) |
| quicktype | 26.0.0 | TypeScript (Node.js 26.13.0) |
| rustc | 1.99.1 | compiles every Rust tool (release profile) |
| schemafy | 0.6.0 | Rust (native) |
| typify | 0.8.0 | Rust (native) |
| valico | 4.0.0 | Rust (native) |

## Category winners

| Tier | Operation | Metric | Winner | Outright wins / contested |
|---|---|---|---|---|
| in-process | codegen | time | schemafy | 8 / 11 |
| in-process | codegen | peak heap | jsonschema_code_generator, schemafy (tied) | 5 each / 11 |
| in-process | schema compile | time | jsonschema-valid | 11 / 12 |
| in-process | schema compile | peak heap | jsonschema-valid | 11 / 12 |
| in-process | validate (valid) | time | jsonschema-macros | 10 / 12 |
| in-process | validate (valid) | peak heap | jsonschemars | 11 / 12 |
| in-process | validate (invalid) | time | jsonschema-macros | 4 / 12 |
| in-process | validate (invalid) | peak heap | jsonschema | 6 / 12 |
| CLI | codegen | time | jsonschemars | 11 / 11 |
| CLI | codegen | peak RSS | jsonschemars | 11 / 11 |
| CLI | validate (valid) | time | jsonschemars | 11 / 12 |
| CLI | validate (valid) | peak RSS | jsonschemars | 11 / 12 |
| CLI | validate (invalid) | time | jsonschemars | 11 / 12 |
| CLI | validate (invalid) | peak RSS | jsonschemars | 11 / 12 |

## In-process (library calls)

### codegen — time

| Fixture | jsonschema_code_generator | jsonschemars | schemafy | typify |
|---|---|---|---|---|
| generated/deep-nesting | 794.830 ± 1.571 µs | 3.110 ± 0.006 ms | **417.222 ± 1.085 µs** | 9.787 ± 0.035 ms |
| generated/large-array | 65.501 ± 0.434 µs | 136.819 ± 0.098 µs | **7.321 ± 0.024 µs** | 1.045 ± 0.002 ms |
| generated/many-defs | 5.974 ± 0.019 ms | 20.741 ± 0.024 ms | **200.033 ± 0.402 µs** | 122.214 ± 1.155 ms |
| generated/wide-object | 8.769 ± 0.075 ms | 18.329 ± 0.164 ms | **8.488 ± 0.008 ms** | 152.159 ± 4.349 ms |
| large/api-catalog | 212.177 ± 0.279 µs | 470.642 ± 1.084 µs | **24.954 ± 0.032 µs** | 3.196 ± 0.096 ms |
| large/circleci | — | 7.156 ± 0.085 ms | **2.172 ± 0.042 ms** | — |
| massive/data-warehouse | **12.401 ± 0.028 ms** | 37.144 ± 0.585 ms | — | 271.038 ± 3.716 ms |
| massive/k8s-deployment | — | 15.960 ± 0.092 ms | 10.166 ± 0.021 ms | **1.828 ± 0.002 ms** |
| medium/package-json | — | 44.994 ± 0.646 ms | — | — |
| medium/user-profile | 90.384 ± 0.235 µs | 188.635 ± 0.483 µs | **33.868 ± 0.152 µs** | 1.436 ± 0.005 ms |
| small/config | 38.584 ± 0.160 µs | 66.002 ± 0.141 µs | **19.222 ± 0.041 µs** | 519.188 ± 0.871 µs |
| small/prettierrc | **243.114 ± 0.663 µs** | 887.362 ± 1.212 µs | 846.110 ± 0.439 µs | — |

### codegen — peak heap

| Fixture | jsonschema_code_generator | jsonschemars | schemafy | typify |
|---|---|---|---|---|
| generated/deep-nesting | **468.85 KiB** | 1.39 MiB | 680.02 KiB | 12.65 MiB |
| generated/large-array | 38.63 KiB | 81.24 KiB | **19.86 KiB** | 1.23 MiB |
| generated/many-defs | 5.97 MiB | 10.85 MiB | **25.63 KiB** | 133.38 MiB |
| generated/wide-object | **4.60 MiB** | 10.44 MiB | 7.73 MiB | 162.36 MiB |
| large/api-catalog | 157.72 KiB | 286.05 KiB | **31.92 KiB** | 3.64 MiB |
| large/circleci | — | 3.95 MiB | **2.43 MiB** | — |
| massive/data-warehouse | **9.03 MiB** | 17.03 MiB | — | 288.90 MiB |
| massive/k8s-deployment | — | 8.48 MiB | 10.36 MiB | **2.79 MiB** |
| medium/package-json | — | 32.87 MiB | — | — |
| medium/user-profile | 59.96 KiB | 116.96 KiB | **40.33 KiB** | 1.71 MiB |
| small/config | **20.63 KiB** | 41.07 KiB | 29.54 KiB | 639.58 KiB |
| small/prettierrc | **197.95 KiB** | 542.19 KiB | 995.53 KiB | — |

### schema compile — time

| Fixture | boon | corvus-json-schema | jsonschema | jsonschema-valid | jsonschemars | valico |
|---|---|---|---|---|---|---|
| generated/deep-nesting | 1.024 ± 0.015 ms | 268.532 ± 0.315 µs | 207.915 ± 0.738 µs | **64.512 ± 0.194 µs** | 104.063 ± 0.324 µs | 1.521 ± 0.010 ms |
| generated/large-array | 44.703 ± 0.139 µs | 27.672 ± 0.214 µs | 20.570 ± 0.105 µs | **4.187 ± 0.029 µs** | 10.689 ± 0.060 µs | 35.378 ± 0.440 µs |
| generated/many-defs | 13.720 ± 0.094 ms | 3.368 ± 0.007 ms | 3.019 ± 0.021 ms | **632.794 ± 7.642 µs** | 1.639 ± 0.004 ms | 4.316 ± 0.074 ms |
| generated/wide-object | 23.424 ± 0.139 ms | 5.898 ± 0.017 ms | 2.510 ± 0.017 ms | **711.746 ± 4.000 µs** | 1.310 ± 0.005 ms | 3.345 ± 0.011 ms |
| large/api-catalog | 233.060 ± 40.503 µs † | 97.061 ± 0.252 µs | 128.375 ± 21.041 µs † | **15.611 ± 0.091 µs** | 53.844 ± 13.275 µs † | 144.643 ± 2.544 µs |
| large/circleci | 3.009 ± 0.013 ms | 1.030 ± 0.001 ms | 1.068 ± 0.006 ms | **214.618 ± 0.507 µs** | 535.792 ± 1.706 µs | 4.241 ± 0.047 ms |
| massive/data-warehouse | 45.688 ± 0.247 ms | 7.051 ± 0.032 ms | 6.457 ± 0.078 ms | **1.193 ± 0.010 ms** | 2.686 ± 0.003 ms | 9.938 ± 0.141 ms |
| massive/k8s-deployment | 9.747 ± 1.961 ms † | 2.384 ± 0.001 ms | 1.613 ± 0.015 ms | **577.996 ± 130.640 µs †** | 764.949 ± 4.299 µs | 9.367 ± 0.780 ms |
| medium/package-json | 72.710 ± 0.647 ms | 10.016 ± 0.034 ms | 15.926 ± 0.508 ms | — | **6.559 ± 0.126 ms** | 18.521 ± 0.326 ms |
| medium/user-profile | 68.606 ± 0.518 µs | 37.526 ± 0.116 µs | 40.446 ± 0.136 µs | **6.071 ± 0.021 µs** | 17.082 ± 0.067 µs | 46.613 ± 0.055 µs |
| small/config | 27.997 ± 0.347 µs | 11.127 ± 0.022 µs | 19.207 ± 0.053 µs | **2.474 ± 0.028 µs** | 7.572 ± 0.023 µs | 20.760 ± 0.039 µs |
| small/prettierrc | 207.664 ± 0.765 µs | 161.480 ± 0.528 µs | 112.145 ± 0.551 µs | **25.037 ± 0.083 µs** | 58.271 ± 0.730 µs | 235.084 ± 1.124 µs |

### schema compile — peak heap

| Fixture | boon | corvus-json-schema | jsonschema | jsonschema-valid | jsonschemars | valico |
|---|---|---|---|---|---|---|
| generated/deep-nesting | 636.04 KiB | 392.96 KiB | 534.41 KiB | **109.92 KiB** | 152.84 KiB | 4.78 MiB |
| generated/large-array | 66.67 KiB | 45.44 KiB | 34.10 KiB | **13.70 KiB** | 20.43 KiB | 111.42 KiB |
| generated/many-defs | 8.10 MiB | 5.83 MiB | 3.88 MiB | **1.95 MiB** | 2.64 MiB | 13.73 MiB |
| generated/wide-object | 13.73 MiB | 7.74 MiB | 4.03 MiB | **1.78 MiB** | 2.69 MiB | 8.89 MiB |
| large/api-catalog | 266.71 KiB | 163.29 KiB | 151.24 KiB | **44.69 KiB** | 68.23 KiB | 378.70 KiB |
| large/circleci | 3.85 MiB | 2.08 MiB | 1.75 MiB | **543.09 KiB** | 869.05 KiB | 9.23 MiB |
| massive/data-warehouse | 16.73 MiB | 12.46 MiB | 9.42 MiB | **2.89 MiB** | 4.04 MiB | 23.94 MiB |
| massive/k8s-deployment | 7.45 MiB | 4.73 MiB | 3.28 MiB | **1.18 MiB** | 1.45 MiB | 20.68 MiB |
| medium/package-json | 29.52 MiB | 16.50 MiB | 21.86 MiB | — | **11.36 MiB** | 47.74 MiB |
| medium/user-profile | 121.82 KiB | 70.45 KiB | 59.13 KiB | **17.31 KiB** | 28.67 KiB | 131.00 KiB |
| small/config | 38.35 KiB | 21.47 KiB | 24.08 KiB | **5.51 KiB** | 10.86 KiB | 44.11 KiB |
| small/prettierrc | 476.85 KiB | 310.60 KiB | 261.04 KiB | **80.78 KiB** | 115.58 KiB | 708.31 KiB |

### validate (valid) — time

| Fixture | boon | corvus-json-schema | jsonschema | jsonschema-macros | jsonschema-valid | jsonschemars | valico |
|---|---|---|---|---|---|---|---|
| generated/deep-nesting | 6.804 ± 0.026 µs | 28.310 ± 0.124 µs | 2.443 ± 0.031 µs | **681.960 ± 25.901 ns** | 19.055 ± 0.072 µs | 3.114 ± 0.025 µs | 564.276 ± 2.199 µs |
| generated/large-array | 18.505 ± 0.109 ms | 72.798 ± 0.939 ms | 7.377 ± 0.098 ms | **2.882 ± 0.196 ms** | 85.576 ± 0.904 ms | 8.891 ± 0.113 ms | 268.233 ± 4.738 ms |
| generated/many-defs | 1.751 ± 0.017 ms | 4.727 ± 0.025 ms | 631.187 ± 1.653 µs | **131.783 ± 8.548 µs** | 326.000 ± 4.432 ms | 746.590 ± 6.562 µs | 39.326 ± 0.334 ms |
| generated/wide-object | 172.210 ± 0.336 µs | 432.143 ± 0.781 µs | 115.473 ± 0.180 µs | 199.233 ± 23.718 µs † | 484.763 ± 3.752 µs | **54.920 ± 0.471 µs** | 1.256 ± 0.013 ms |
| large/api-catalog | 580.173 ± 3.362 ns | 2.221 ± 0.018 µs | 245.219 ± 1.132 ns | **109.381 ± 11.659 ns †** | 28.195 ± 0.092 µs | 433.691 ± 7.340 ns | 6.549 ± 0.010 µs |
| large/circleci | 2.240 ± 0.023 µs | 12.294 ± 0.015 µs | 474.980 ± 2.675 ns | **236.058 ± 14.091 ns** | 166.885 ± 0.621 µs | 1.023 ± 0.014 µs | 20.187 ± 0.051 µs |
| massive/data-warehouse | 71.561 ± 0.494 µs | 55.788 ± 0.346 ms | 24.578 ± 0.119 µs | **8.663 ± 0.463 µs** | 26.274 ± 0.176 ms | 46.720 ± 0.464 µs | 185.509 ± 0.730 ms |
| massive/k8s-deployment | 529.185 ± 0.974 ns | 2.568 ± 0.016 µs | 219.242 ± 3.692 ns | **63.008 ± 2.903 ns** | 3.113 ± 0.035 µs | 506.684 ± 2.407 ns | 7.975 ± 0.031 µs |
| medium/package-json | 546.024 ± 2.819 ns | 2.857 ± 0.021 µs | **234.595 ± 1.270 ns** | 872.485 ± 20.420 ns | — | 494.336 ± 0.845 ns | 5.700 ± 0.012 µs |
| medium/user-profile | 860.577 ± 4.557 ns | 3.416 ± 0.021 µs | 380.560 ± 2.901 ns | **202.855 ± 8.059 ns** | 13.235 ± 0.092 µs | 504.606 ± 4.807 ns | 8.814 ± 0.051 µs |
| small/config | 227.478 ± 0.629 ns | 922.011 ± 6.946 ns | 116.016 ± 0.897 ns | **73.515 ± 2.032 ns** | 9.343 ± 0.047 µs | 141.136 ± 3.515 ns | 1.882 ± 0.007 µs |
| small/prettierrc | 746.337 ± 5.525 ns | 4.948 ± 0.237 µs | 176.402 ± 3.007 ns | **59.971 ± 3.155 ns** | 8.559 ± 0.028 µs | 284.722 ± 2.927 ns | 7.452 ± 0.020 µs |

### validate (valid) — peak heap

| Fixture | boon | corvus-json-schema | jsonschema | jsonschema-macros | jsonschema-valid | jsonschemars | valico |
|---|---|---|---|---|---|---|---|
| generated/deep-nesting | 2.25 KiB | 23.40 KiB | 0.03 KiB | 0.03 KiB | 13.29 KiB | **0.00 KiB** | 37.75 KiB |
| generated/large-array | 0.19 KiB | 1.49 KiB | 0.03 KiB | 0.03 KiB | 2.05 KiB | **0.00 KiB** | 39.97 MiB |
| generated/many-defs | 0.56 KiB | 2.29 KiB | 0.03 KiB | 0.03 KiB | 10.12 KiB | **0.00 KiB** | 3.34 MiB |
| generated/wide-object | 0.19 KiB | 1.19 KiB | 0.03 KiB | 0.03 KiB | 0.76 KiB | **0.00 KiB** | 236.68 KiB |
| large/api-catalog | 0.19 KiB | 1.48 KiB | 0.03 KiB | 0.03 KiB | 10.41 KiB | **0.00 KiB** | 1.43 KiB |
| large/circleci | 2.58 KiB | 3.26 KiB | 0.03 KiB | 0.03 KiB | 328.25 KiB | 0.22 KiB | 3.41 KiB |
| massive/data-warehouse | 0.19 KiB | 1.48 KiB | 0.03 KiB | 0.03 KiB | 12.55 KiB | **0.00 KiB** | 155.85 KiB |
| massive/k8s-deployment | 0.19 KiB | 1.59 KiB | 0.03 KiB | 0.03 KiB | 2.07 KiB | **0.00 KiB** | 1.57 KiB |
| medium/package-json | 0.19 KiB | 1.23 KiB | 0.03 KiB | 0.03 KiB | — | **0.00 KiB** | 1.20 KiB |
| medium/user-profile | 0.19 KiB | 1.23 KiB | 0.03 KiB | 0.03 KiB | 8.74 KiB | **0.00 KiB** | 1.88 KiB |
| small/config | 0.19 KiB | 1.18 KiB | 0.03 KiB | 0.03 KiB | 10.18 KiB | **0.00 KiB** | 0.60 KiB |
| small/prettierrc | 0.77 KiB | 1.49 KiB | 0.03 KiB | 0.03 KiB | 2.05 KiB | **0.00 KiB** | 1.71 KiB |

### validate (invalid) — time

| Fixture | boon | corvus-json-schema | jsonschema | jsonschema-macros | jsonschema-valid | jsonschemars | valico |
|---|---|---|---|---|---|---|---|
| generated/deep-nesting | 7.849 ± 0.022 µs | 36.327 ± 2.316 µs | **2.834 ± 0.019 µs** | 16.058 ± 0.662 µs | 23.483 ± 0.052 µs | 5.227 ± 0.034 µs | 565.512 ± 0.651 µs |
| generated/large-array | 18.407 ± 0.156 ms | 78.610 ± 2.167 ms | 7.342 ± 0.058 ms | **5.838 ± 0.229 ms** | 84.450 ± 0.255 ms | 8.737 ± 0.071 ms | 258.586 ± 3.149 ms |
| generated/many-defs | 1.755 ± 0.009 ms | 5.151 ± 0.144 ms | 629.707 ± 5.171 µs | **160.363 ± 7.486 µs** | 319.199 ± 0.971 ms | 751.257 ± 1.665 µs | 39.203 ± 0.277 ms |
| generated/wide-object | 172.983 ± 1.324 µs | 469.963 ± 16.869 µs | 110.045 ± 0.315 µs | 211.258 ± 25.967 µs † | 485.974 ± 1.634 µs | **55.322 ± 0.096 µs** | 1.260 ± 0.006 ms |
| large/api-catalog | 671.171 ± 3.729 ns | 2.661 ± 0.089 µs | 399.402 ± 2.356 ns | **359.749 ± 12.977 ns** | 20.931 ± 0.044 µs | 1.059 ± 0.009 µs | 4.335 ± 0.005 µs |
| large/circleci | **128.570 ± 0.290 ns** | 632.550 ± 71.545 ns † | 133.056 ± 0.698 ns | 168.962 ± 8.703 ns | 553.610 ± 3.879 ns | 275.919 ± 1.985 ns | 462.483 ± 1.626 ns |
| massive/data-warehouse | 185.008 ± 0.758 µs | 421.177 ± 19.858 µs | 141.288 ± 0.538 µs | **123.660 ± 7.320 µs** | 44.098 ± 0.251 ms | 277.862 ± 1.290 µs | 891.729 ± 1.943 µs |
| massive/k8s-deployment | 179.560 ± 1.684 ns | 801.367 ± 31.185 ns | 136.981 ± 0.504 ns | 144.959 ± 7.497 ns | 236.147 ± 0.268 µs | 247.770 ± 2.320 ns | 916.550 ± 1.278 ns |
| medium/package-json | 176.392 ± 1.011 ns | 864.319 ± 30.036 ns | **154.211 ± 0.222 ns** | 424.310 ± 23.721 ns | — | 252.945 ± 1.914 ns | 848.980 ± 3.540 ns |
| medium/user-profile | 373.297 ± 1.011 ns | 2.040 ± 0.082 µs | 314.800 ± 1.212 ns | 315.699 ± 13.388 ns | 8.202 ± 0.026 µs | 558.142 ± 5.140 ns | 2.405 ± 0.005 µs |
| small/config | 294.637 ± 1.529 ns | 1.667 ± 0.042 µs | 250.975 ± 0.561 ns | 254.992 ± 13.642 ns | 9.747 ± 0.036 µs | 544.235 ± 5.817 ns | 1.680 ± 0.007 µs |
| small/prettierrc | **534.133 ± 2.281 ns** | 2.823 ± 0.090 µs | 913.073 ± 9.247 ns | 919.710 ± 46.568 ns | 5.254 ± 0.024 µs | 1.534 ± 0.006 µs | 2.027 ± 0.012 µs |

### validate (invalid) — peak heap

| Fixture | boon | corvus-json-schema | jsonschema | jsonschema-macros | jsonschema-valid | jsonschemars | valico |
|---|---|---|---|---|---|---|---|
| generated/deep-nesting | 2.97 KiB | 58.76 KiB | **0.75 KiB** | 1.56 KiB | 19.65 KiB | 5.25 KiB | 38.21 KiB |
| generated/large-array | 1.37 KiB | 2.97 KiB | **0.46 KiB** | 0.48 KiB | 2.75 KiB | 0.84 KiB | 33.72 MiB |
| generated/many-defs | 7.77 KiB | 7.67 KiB | 0.77 KiB | **0.53 KiB** | 11.02 KiB | 2.25 KiB | 3.34 MiB |
| generated/wide-object | 0.74 KiB | 1.64 KiB | **0.41 KiB** | 0.45 KiB | 1.54 KiB | 0.76 KiB | 236.68 KiB |
| large/api-catalog | 1.44 KiB | 3.04 KiB | 0.83 KiB | 0.85 KiB | 11.63 KiB | 2.01 KiB | 1.21 KiB |
| large/circleci | 0.74 KiB | 1.52 KiB | 0.40 KiB | 0.44 KiB | 1.63 KiB | 0.93 KiB | **0.30 KiB** |
| massive/data-warehouse | 467.89 KiB | **2.40 KiB** | 111.78 KiB | 93.05 KiB | 159.79 KiB | 247.91 KiB | 127.87 KiB |
| massive/k8s-deployment | 0.81 KiB | 1.48 KiB | **0.48 KiB** | 0.52 KiB | 1.10 MiB | 0.75 KiB | 0.64 KiB |
| medium/package-json | 0.74 KiB | 1.52 KiB | **0.40 KiB** | 0.44 KiB | — | 0.75 KiB | 0.48 KiB |
| medium/user-profile | 1.30 KiB | 2.52 KiB | **0.73 KiB** | 0.82 KiB | 9.73 KiB | 1.48 KiB | 0.76 KiB |
| small/config | 1.30 KiB | 2.51 KiB | 0.71 KiB | 0.80 KiB | 11.52 KiB | 1.60 KiB | **0.69 KiB** |
| small/prettierrc | 2.38 KiB | 2.07 KiB | 2.69 KiB | 2.76 KiB | 4.50 KiB | 3.27 KiB | **1.74 KiB** |

## CLI (end to end)

### codegen — time

| Fixture | datamodel-code-generator | json-schema-to-typescript | jsonschemars | quicktype | typify |
|---|---|---|---|---|---|
| *startup* | 266.574 ± 5.960 ms | 192.957 ± 7.041 ms | 2.298 ± 1.181 ms † | 340.145 ± 12.132 ms | 113.677 ± 6.439 ms |
| generated/deep-nesting | 231.982 ± 2.599 ms | 172.348 ± 0.844 ms | **6.389 ± 0.257 ms** | 295.773 ± 4.915 ms | 110.985 ± 0.796 ms |
| generated/large-array | 223.314 ± 1.775 ms | 162.477 ± 1.289 ms | **2.257 ± 0.063 ms** | 277.714 ± 1.787 ms | 90.125 ± 1.405 ms |
| generated/many-defs | 467.927 ± 2.941 ms | 241.268 ± 1.433 ms | **28.615 ± 0.488 ms** | 448.565 ± 4.803 ms | 447.908 ± 3.322 ms |
| generated/wide-object | 610.796 ± 4.769 ms | 288.749 ± 1.876 ms | **25.359 ± 0.142 ms** | 473.115 ± 1.997 ms | 517.290 ± 3.299 ms |
| large/api-catalog | 231.272 ± 1.132 ms | 167.108 ± 1.117 ms | **2.807 ± 0.038 ms** | 289.296 ± 2.399 ms | 96.521 ± 2.076 ms |
| large/circleci | 297.580 ± 3.328 ms | 201.263 ± 1.098 ms | **11.503 ± 0.395 ms** | — | — |
| massive/data-warehouse | 1.165 ± 0.021 s | 351.962 ± 3.213 ms | **47.714 ± 1.201 ms** | 655.712 ± 3.858 ms | 846.734 ± 4.221 ms |
| massive/k8s-deployment | 502.886 ± 13.149 ms | 290.352 ± 1.986 ms | **24.244 ± 0.498 ms** | 453.225 ± 5.941 ms | 100.103 ± 1.616 ms |
| medium/package-json | — | — | 106.542 ± 1.419 ms | — | — |
| medium/user-profile | 244.071 ± 1.922 ms | 168.929 ± 1.121 ms | **2.627 ± 0.104 ms** | 292.455 ± 3.141 ms | 96.740 ± 1.409 ms |
| small/config | 235.080 ± 2.343 ms | 164.509 ± 1.321 ms | **2.376 ± 0.130 ms** | 287.030 ± 2.097 ms | 92.575 ± 0.841 ms |
| small/prettierrc | 265.254 ± 5.230 ms | 171.412 ± 1.250 ms | **3.844 ± 0.218 ms** | 294.965 ± 4.873 ms | — |

### codegen — peak RSS

| Fixture | datamodel-code-generator | json-schema-to-typescript | jsonschemars | quicktype | typify |
|---|---|---|---|---|---|
| *startup* | 59.91 ± 0.19 MiB | 75.41 ± 0.29 MiB | 3.70 ± 0.01 MiB | 129.03 ± 0.11 MiB | 31.66 ± 0.02 MiB |
| generated/deep-nesting | 61.67 ± 0.29 MiB | 78.09 ± 0.05 MiB | **7.56 ± 0.01 MiB** | 168.50 MiB | 37.92 ± 0.02 MiB |
| generated/large-array | 60.72 ± 0.16 MiB | 75.20 ± 0.33 MiB | **4.66 ± 0.02 MiB** | 130.41 ± 0.17 MiB | 33.17 ± 0.01 MiB |
| generated/many-defs | 83.05 ± 0.12 MiB | 122.52 MiB | **22.20 ± 0.10 MiB** | 218.08 ± 6.60 MiB | 98.05 ± 0.02 MiB |
| generated/wide-object | 98.08 ± 0.38 MiB | 167.27 MiB | **23.31 ± 0.20 MiB** | 231.56 ± 7.75 MiB | 115.20 ± 0.53 MiB |
| large/api-catalog | 62.75 ± 0.47 MiB | 77.02 MiB | **5.36 MiB** | 142.86 MiB | 34.58 MiB |
| large/circleci | 69.70 ± 0.04 MiB | 90.94 ± 0.14 MiB | **11.27 ± 0.05 MiB** | — | — |
| massive/data-warehouse | 176.44 ± 0.02 MiB | 179.42 ± 0.47 MiB | **30.75 ± 0.47 MiB** | 226.89 ± 0.10 MiB | 176.77 MiB |
| massive/k8s-deployment | 87.27 ± 0.18 MiB | 132.39 ± 0.26 MiB | **17.92 MiB** | 203.03 MiB | 31.67 ± 0.01 MiB |
| medium/package-json | — | — | 42.61 ± 0.43 MiB | — | — |
| medium/user-profile | 60.42 ± 0.08 MiB | 75.02 MiB | **4.88 ± 0.01 MiB** | 133.86 ± 0.33 MiB | 33.39 ± 0.02 MiB |
| small/config | 59.69 ± 0.34 MiB | 75.12 ± 0.21 MiB | **4.38 ± 0.01 MiB** | 128.11 MiB | 32.78 ± 0.01 MiB |
| small/prettierrc | 66.52 ± 0.03 MiB | 76.86 ± 0.04 MiB | **5.58 ± 0.01 MiB** | 148.56 ± 0.05 MiB | — |

### validate (valid) — time

| Fixture | ajv | blaze | boon | fastjsonschema | jsonschema-cli | jsonschemars | python-jsonschema |
|---|---|---|---|---|---|---|---|
| *startup* | 142.718 ± 5.824 ms | 7.304 ± 1.622 ms † | 6.231 ± 0.646 ms † | 25.465 ± 0.312 ms | 7.285 ± 1.817 ms † | 3.648 ± 0.394 ms † | 75.217 ± 4.442 ms |
| generated/deep-nesting | 138.818 ± 0.727 ms | 14.675 ± 0.304 ms | 4.422 ± 0.415 ms | 97.573 ± 0.963 ms | 4.459 ± 0.268 ms | **1.950 ± 0.037 ms** | 75.948 ± 0.708 ms |
| generated/large-array | 159.562 ± 0.883 ms | 101.058 ± 1.304 ms | 59.581 ± 0.678 ms | 148.791 ± 1.134 ms | 45.385 ± 0.352 ms | 45.052 ± 0.598 ms | 1.404 ± 0.012 s |
| generated/many-defs | 244.753 ± 1.665 ms | 117.706 ± 0.621 ms | 23.516 ± 0.453 ms | 391.116 ± 4.483 ms | 11.741 ± 0.189 ms | **7.671 ± 0.229 ms** | 422.581 ± 2.527 ms |
| generated/wide-object | — | 59.940 ± 0.478 ms | 28.538 ± 0.512 ms | 310.683 ± 1.676 ms | 8.418 ± 0.110 ms | **4.005 ± 0.034 ms** | 301.075 ± 1.871 ms |
| large/api-catalog | 127.722 ± 0.926 ms | 4.994 ± 0.035 ms | 3.275 ± 0.117 ms | 37.258 ± 0.541 ms | 4.224 ± 0.446 ms † | **1.859 ± 0.031 ms** | 66.756 ± 0.615 ms |
| large/circleci | 166.143 ± 0.862 ms | 28.772 ± 0.196 ms | 7.047 ± 0.134 ms | 329.212 ± 20.320 ms | 6.066 ± 0.233 ms | **2.694 ± 0.084 ms** | 76.096 ± 2.575 ms |
| massive/data-warehouse | 621.817 ± 6.590 ms | 269.817 ± 2.647 ms | 54.781 ± 1.614 ms | 1.313 ± 0.004 s | 13.645 ± 0.612 ms | **6.237 ± 0.363 ms** | 493.886 ± 3.719 ms |
| massive/k8s-deployment | — | — | 13.748 ± 0.344 ms | 399.419 ± 4.454 ms | 284.077 ± 79.380 ms † | **3.444 ± 0.205 ms** | 240.494 ± 2.578 ms |
| medium/package-json | 939.199 ± 33.474 ms | 416.093 ± 5.321 ms | 83.272 ± 0.953 ms | 908.508 ± 3.660 ms | — | **8.824 ± 0.692 ms** | 77.557 ± 1.344 ms |
| medium/user-profile | 128.892 ± 0.864 ms | 4.114 ± 0.270 ms | 3.381 ± 0.109 ms | 32.276 ± 0.522 ms | 4.586 ± 0.204 ms | **1.985 ± 0.121 ms** | 71.062 ± 1.010 ms |
| small/config | 127.266 ± 0.870 ms | 3.822 ± 0.156 ms | 3.280 ± 0.158 ms | 28.952 ± 1.443 ms | 4.461 ± 0.207 ms | **1.890 ± 0.100 ms** | 63.428 ± 2.573 ms |
| small/prettierrc | 125.375 ± 1.004 ms | 6.014 ± 0.254 ms | 3.296 ± 0.091 ms | 39.173 ± 0.333 ms | 4.237 ± 0.312 ms | **1.852 ± 0.056 ms** | 65.750 ± 2.703 ms |

### validate (valid) — peak RSS

| Fixture | ajv | blaze | boon | fastjsonschema | jsonschema-cli | jsonschemars | python-jsonschema |
|---|---|---|---|---|---|---|---|
| *startup* | 53.89 ± 0.03 MiB | 8.50 MiB | 5.67 MiB | 17.19 ± 0.02 MiB | 8.45 ± 0.01 MiB | 3.47 MiB | 25.00 ± 0.01 MiB |
| generated/deep-nesting | 62.64 ± 0.10 MiB | 18.09 MiB | 6.98 ± 0.00 MiB | 88.97 MiB | 10.03 ± 0.01 MiB | **4.16 ± 0.01 MiB** | 25.02 ± 0.03 MiB |
| generated/large-array | 84.23 ± 0.66 MiB | 300.80 MiB | **55.89 MiB** | 60.25 ± 0.03 MiB | 64.92 MiB | 59.69 MiB | 65.62 ± 0.08 MiB |
| generated/many-defs | 80.34 ± 0.43 MiB | 84.83 MiB | 20.59 MiB | 327.05 ± 0.01 MiB | 21.62 ± 0.02 MiB | **13.19 ± 0.02 MiB** | 28.28 ± 0.06 MiB |
| generated/wide-object | — | 26.84 ± 0.01 MiB | 15.20 ± 0.01 MiB | 281.48 ± 1.17 MiB | 15.50 MiB | **7.34 MiB** | 26.67 ± 0.14 MiB |
| large/api-catalog | 55.55 MiB | 9.56 ± 0.01 MiB | 6.00 ± 0.01 MiB | 27.38 ± 0.01 MiB | 10.58 MiB | **3.97 MiB** | 24.92 MiB |
| large/circleci | 75.36 MiB | 19.53 ± 0.01 MiB | 9.23 ± 0.02 MiB | 270.12 ± 0.63 MiB | 13.27 ± 0.02 MiB | **5.48 ± 0.01 MiB** | 25.34 ± 0.01 MiB |
| massive/data-warehouse | 156.66 ± 0.86 MiB | 297.67 MiB | 23.66 MiB | 971.75 ± 0.10 MiB | 21.38 ± 0.08 MiB | **9.50 ± 0.26 MiB** | 27.55 ± 0.12 MiB |
| massive/k8s-deployment | — | — | 13.38 ± 0.10 MiB | 318.50 ± 0.47 MiB | 20.41 ± 0.08 MiB | **6.03 ± 0.01 MiB** | 26.47 ± 0.07 MiB |
| medium/package-json | 362.58 ± 0.36 MiB | 68.64 MiB | 29.02 ± 0.04 MiB | 659.86 ± 0.37 MiB | — | **11.61 ± 0.06 MiB** | 28.08 ± 0.04 MiB |
| medium/user-profile | 54.61 ± 0.02 MiB | 9.22 MiB | 5.89 ± 0.01 MiB | 20.47 MiB | 10.34 ± 0.02 MiB | **3.83 MiB** | 24.69 ± 0.00 MiB |
| small/config | 54.30 ± 0.03 MiB | 8.91 MiB | 5.86 ± 0.03 MiB | 18.00 ± 0.01 MiB | 10.17 MiB | **3.73 MiB** | 24.72 MiB |
| small/prettierrc | 57.95 ± 0.02 MiB | 10.11 ± 0.00 MiB | 5.94 MiB | 30.06 ± 0.01 MiB | 9.19 MiB | **3.91 ± 0.01 MiB** | 24.89 ± 0.11 MiB |

### validate (invalid) — time

| Fixture | ajv | blaze | boon | fastjsonschema | jsonschema-cli | jsonschemars | python-jsonschema |
|---|---|---|---|---|---|---|---|
| *startup* | 145.417 ± 4.002 ms | 7.397 ± 1.043 ms † | 6.171 ± 1.276 ms † | 25.461 ± 0.372 ms | 5.293 ± 1.265 ms † | 3.715 ± 0.613 ms † | 70.817 ± 9.022 ms † |
| generated/deep-nesting | 139.268 ± 1.347 ms | 36.306 ± 0.205 ms | 4.424 ± 0.092 ms | 97.269 ± 0.591 ms | 4.358 ± 0.055 ms | **2.027 ± 0.118 ms** | 76.003 ± 0.733 ms |
| generated/large-array | 159.943 ± 1.938 ms | 116.319 ± 0.584 ms | 59.687 ± 0.285 ms | 149.147 ± 1.172 ms | 45.601 ± 0.538 ms | 45.087 ± 0.737 ms | 1.422 ± 0.012 s |
| generated/many-defs | 244.109 ± 0.878 ms | 122.680 ± 0.689 ms | 23.365 ± 0.251 ms | 391.751 ± 1.637 ms | 11.968 ± 0.321 ms | **7.597 ± 0.054 ms** | 425.289 ± 3.370 ms |
| generated/wide-object | — | 59.623 ± 0.480 ms | 28.543 ± 0.231 ms | 310.841 ± 2.719 ms | 8.428 ± 0.041 ms | **4.048 ± 0.141 ms** | 301.839 ± 2.750 ms |
| large/api-catalog | 128.567 ± 1.570 ms | 5.638 ± 0.168 ms | 3.269 ± 0.119 ms | 37.635 ± 0.859 ms | 4.217 ± 0.067 ms | **1.884 ± 0.085 ms** | 66.915 ± 0.348 ms |
| large/circleci | 169.294 ± 4.578 ms | 28.819 ± 0.284 ms | 7.002 ± 0.106 ms | 339.374 ± 11.018 ms | 5.918 ± 0.379 ms | **2.770 ± 0.149 ms** | 75.819 ± 1.371 ms |
| massive/data-warehouse | 551.824 ± 4.239 ms | 333.619 ± 2.165 ms | 57.064 ± 2.252 ms | 1.318 ± 0.010 s | 13.522 ± 0.512 ms | **5.933 ± 0.645 ms †** | 499.654 ± 3.745 ms |
| massive/k8s-deployment | — | — | 13.439 ± 0.339 ms | 395.132 ± 4.131 ms | 268.012 ± 36.615 ms † | **3.457 ± 0.264 ms** | 247.433 ± 2.742 ms |
| medium/package-json | 950.139 ± 40.147 ms | 397.755 ± 3.971 ms | 84.710 ± 1.228 ms | 905.842 ± 3.240 ms | — | **8.113 ± 0.751 ms** | 78.353 ± 1.173 ms |
| medium/user-profile | 129.968 ± 0.886 ms | 4.579 ± 0.204 ms | 3.430 ± 0.134 ms | 32.282 ± 0.649 ms | 4.521 ± 0.169 ms | **2.014 ± 0.159 ms** | 70.072 ± 0.620 ms |
| small/config | 129.794 ± 1.550 ms | 3.968 ± 0.122 ms | 3.313 ± 0.054 ms | 31.303 ± 1.280 ms | 4.411 ± 0.207 ms | **2.129 ± 0.132 ms** | 64.215 ± 2.114 ms |
| small/prettierrc | 130.066 ± 2.758 ms | 6.380 ± 0.165 ms | 3.260 ± 0.043 ms | 39.183 ± 0.329 ms | 4.069 ± 0.199 ms | **1.913 ± 0.117 ms** | 63.913 ± 0.857 ms |

### validate (invalid) — peak RSS

| Fixture | ajv | blaze | boon | fastjsonschema | jsonschema-cli | jsonschemars | python-jsonschema |
|---|---|---|---|---|---|---|---|
| *startup* | 54.11 ± 0.15 MiB | 8.67 MiB | 5.69 ± 0.02 MiB | 17.22 MiB | 8.56 ± 0.00 MiB | 3.53 MiB | 25.41 ± 0.18 MiB |
| generated/deep-nesting | 62.59 ± 0.06 MiB | 18.25 ± 0.01 MiB | 7.03 MiB | 87.38 MiB | 10.16 ± 0.01 MiB | **4.23 ± 0.02 MiB** | 25.06 MiB |
| generated/large-array | 84.34 MiB | 326.53 MiB | **55.94 MiB** | 60.22 MiB | 65.00 ± 0.01 MiB | 59.72 ± 0.01 MiB | 65.66 ± 0.02 MiB |
| generated/many-defs | 81.64 MiB | 85.44 MiB | 19.23 MiB | 327.55 MiB | 21.69 ± 0.01 MiB | **13.27 ± 0.04 MiB** | 28.42 ± 0.07 MiB |
| generated/wide-object | — | 27.56 MiB | 15.27 ± 0.02 MiB | 283.61 MiB | 15.64 MiB | **7.39 ± 0.18 MiB** | 26.58 ± 0.16 MiB |
| large/api-catalog | 55.64 ± 0.04 MiB | 9.70 MiB | 6.03 ± 0.00 MiB | 27.42 ± 0.10 MiB | 10.69 ± 0.01 MiB | **3.98 ± 0.01 MiB** | 24.69 ± 0.01 MiB |
| large/circleci | 75.36 ± 0.05 MiB | 19.48 MiB | 9.22 ± 0.26 MiB | 271.75 MiB | 13.30 ± 0.13 MiB | **5.56 MiB** | 25.30 ± 0.10 MiB |
| massive/data-warehouse | 135.41 ± 0.27 MiB | 68.66 MiB | 23.88 ± 0.25 MiB | 972.09 ± 0.14 MiB | 21.33 MiB | **10.14 ± 0.20 MiB** | 28.22 ± 0.05 MiB |
| massive/k8s-deployment | — | — | 13.27 ± 0.60 MiB | 318.83 ± 1.68 MiB | 20.41 ± 0.07 MiB | **6.00 ± 0.01 MiB** | 26.36 ± 0.02 MiB |
| medium/package-json | 362.45 ± 0.02 MiB | 68.72 MiB | 29.14 ± 0.05 MiB | 658.38 ± 0.84 MiB | — | **11.73 MiB** | 28.16 MiB |
| medium/user-profile | 54.78 ± 0.08 MiB | 9.31 MiB | 5.89 ± 0.02 MiB | 20.50 MiB | 10.48 MiB | **3.86 MiB** | 24.64 ± 0.03 MiB |
| small/config | 54.59 ± 0.14 MiB | 9.06 MiB | 5.88 ± 0.01 MiB | 18.12 MiB | 10.31 ± 0.02 MiB | **3.81 ± 0.01 MiB** | 24.64 ± 0.01 MiB |
| small/prettierrc | 58.22 ± 0.11 MiB | 10.19 MiB | 5.98 MiB | 30.06 ± 0.01 MiB | 9.36 ± 0.02 MiB | **3.98 ± 0.00 MiB** | 24.67 ± 0.05 MiB |

## Compatibility

509 of 535 attempted cells pass. Rows with at least one failure:

| Tier | Operation | Fixture | ajv | blaze | boon | corvus-json-schema | datamodel-code-generator | fastjsonschema | json-schema-to-typescript | jsonschema | jsonschema-cli | jsonschema-macros | jsonschema-valid | jsonschema_code_generator | jsonschemars | python-jsonschema | quicktype | schemafy | typify | valico |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| in-process | codegen | large/circleci | — | — | — | — | — | — | — | — | — | — | — | FAIL: panic: Could not parse large/circleci/schema.json: expected value at line 1027 column 29 | pass | — | — | pass | FAIL: panic: not implemented: if/then/else schemas are not supported: { "if": { "required": [ "type" ], "properties": { "type": { "const": "release" } } }, "then": { "required": [ "plan_name" ], "properties": { "plan_name": { "type": "string", "markdownDescription": "Required plan name for release jobs." } }, "additionalProperties": true, "markdownDescription": "https://circleci.com/docs/reference/configuration-reference/#job-type\n\nRelease job." }, "else": { "if": { "required": [ "type" ], "properties": { "type": { "enum": [ "lock", "unlock" ] } } }, "then": { "required": [ "key" ], "properties": { "key": { "type": "string", "markdownDescription": "Required key for lock/unlock jobs." } }, "additionalProperties": true, "markdownDescription": "https://circleci.com/docs/reference/configuration-reference/#job-type\n\nA lock/unlock job" }, "else": { "if": { "required": [ "type" ], "properties": { "type": { "enum": [ "approval", "no-op" ] } } }, "then": { "additionalProperties": true, "markdownDescription": "https://circleci.com/docs/reference/configuration-reference/#job-type\n\nApproval/no-op jobs primarily act as workflow markers. Other keys like `steps:` are permitted but will be ignored in the final pipeline." }, "else": { "if": { "anyOf": [ { "not": { "required": [ "type" ] } }, { "required": [ "type" ], "properties": { "type": { "const": "build" } } } ], "markdownDescription": "https://circleci.com/docs/reference/configuration-reference/#job-type\n\nA build job. The job explicitly has type 'build' or defaults to build when no type specified." }, "then": { "anyOf": [ { "type": "object", "required": [ "executor" ], "markdownDescription": "A job that uses an executor defined elsewhere in the configuration." }, { "oneOf": [ { "type": "object", "required": [ "machine" ] }, { "type": "object", "required": [ "docker" ] }, { "type": "object", "required": [ "macos" ] } ] } ], "required": [ "steps" ], "properties": { "circleci_ip_ranges": { "type": "boolean", "markdownDescription": "Enable running jobs with a set of well-defined IP address ranges." }, "description": { "type": "string" }, "docker": { "type": "array", "items": { "type": "object", "required": [ "image" ], "properties": { "auth": { "type": "object", "required": [ "password", "username" ], "properties": { "password": { "type": "string" }, "username": { "type": "string" } }, "additionalProperties": false }, "aws_auth": { "oneOf": [ { "type": "object", "required": [ "aws_access_key_id", "aws_secret_access_key" ], "properties": { "aws_access_key_id": { "type": "string" }, "aws_secret_access_key": { "type": "string" } }, "additionalProperties": false }, { "type": "object", "required": [ "oidc_role_arn" ], "properties": { "oidc_role_arn": { "type": "string" } }, "additionalProperties": false } ] }, "command": { "oneOf": [ { "type": "string" }, { "type": "array", "items": { "type": "string" } } ] }, "entrypoint": { "oneOf": [ { "type": "string" }, { "type": "array", "items": { "type": "string" } } ] }, "environment": { "oneOf": [ { "type": [ "string", "null" ], "markdownDescription": "A map of environment variable names and values." }, { "type": "array", "items": { "$ref": "#/definitions/environment" }, "markdownDescription": "An array of strings in the form KEY=VALUE" }, { "type": "object", "additionalProperties": { "type": [ "string", "boolean", "number", "null" ] }, "markdownDescription": "The value should be a string, but we allow null, boolean and numbers too. Examples: `FOO: true` and `BAR: 2` `BAZ:`" } ] }, "image": { "type": "string" }, "name": { "type": "string" }, "user": { "type": "string" } }, "additionalProperties": false }, "minItems": 1 }, "environment": { "oneOf": [ { "type": [ "string", "null" ], "markdownDescription": "A map of environment variable names and values." }, { "type": "array", "items": { "$ref": "#/definitions/environment" }, "markdownDescription": "An array of strings in the form KEY=VALUE" }, { "type": "object", "additionalProperties": { "type": [ "string", "boolean", "number", "null" ] }, "markdownDescription": "The value should be a string, but we allow null, boolean and numbers too. Examples: `FOO: true` and `BAR: 2` `BAZ:`" } ] }, "executor": { "oneOf": [ { "type": "string", "markdownDescription": "https://circleci.com/docs/reference/reusing-config/#executor\n\nThe name of the executor to use (defined via the top level executors map or from an orb)." }, { "type": "object", "minProperties": 1, "required": [ "name" ], "properties": { "name": { "type": "string" } }, "additionalProperties": true, "markdownDescription": "Executor stanza to use for the job, with optional parameter overrides." } ] }, "machine": { "oneOf": [ { "anyOf": [ { "type": "string", "pattern": " *<< *parameters.([^ ]+) *>> *" }, { "type": "boolean" }, { "type": "number" }, { "type": "string" } ] }, { "type": "object", "properties": { "docker_layer_caching": { "anyOf": [ { "type": "string", "pattern": " *<< *parameters.([^ ]+) *>> *" }, { "type": "boolean" }, { "type": "number" }, { "type": "string" } ], "markdownDescription": "Set to `true` to enable [Docker Layer Caching](https://circleci.com/docs/docker-layer-caching). Note: If you haven't already, you must open a support ticket to have a CircleCI Sales representative contact you about enabling this feature on your account for an additional fee." }, "enabled": { "anyOf": [ { "type": "string", "pattern": " *<< *parameters.([^ ]+) *>> *" }, { "type": "boolean" }, { "type": "number" }, { "type": "string" } ], "markdownDescription": "A boolean or a template parameter evaluating to a boolean" }, "image": { "type": "string" }, "resource_class": { "type": "string" }, "shell": { "type": "string" } }, "additionalProperties": false } ], "markdownDescription": "Use the default machine executor image" }, "macos": { "type": "object", "required": [ "xcode" ], "properties": { "resource_class": { "type": "string" }, "shell": { "type": "string" }, "xcode": { "type": [ "string", "number" ] } }, "additionalProperties": false }, "parallelism": { "anyOf": [ { "oneOf": [ { "type": "string", "pattern": " *<< *parameters.([^ ]+) *>> *" }, { "type": "string", "pattern": " *<< *pipeline.parameters.([^ ]+) *>> *" } ] }, { "type": "integer" } ], "markdownDescription": "A integer or a parameter evaluating to a integer" }, "parameters": { "type": "object", "minProperties": 1, "additionalProperties": { "type": "object", "required": [ "type" ], "properties": { "default": { "oneOf": [ { "type": "string" }, { "type": "boolean" }, { "type": "integer" }, { "type": "array", "items": { "$ref": "#/definitions/step" } } ] }, "description": { "type": "string", "markdownDescription": "A description of the parameter" }, "enum": { "type": "array", "items": { "type": "string" }, "minItems": 1, "markdownDescription": "List of allowed values for an `enum` type parameter" }, "type": { "type": "string", "enum": [ "boolean", "string", "steps", "enum", "executor", "integer", "env_var_name" ], "markdownDescription": "The parameter type. See [Parameter Types](https://circleci.com/docs/reusing-config#parameter-syntax) for details:\n- `boolean`: A boolean parameter\n- `string`: A string parameter \n- `integer`: An integer parameter\n- `enum`: An enum parameter (list of allowed string values)\n- `executor`: An executor parameter (allows job to choose executor)\n- `steps`: A steps parameter (allows mixing predefined and user-defined steps)\n- `env_var_name`: An environment variable name parameter (must match POSIX_NAME regexp)" } }, "additionalProperties": false }, "markdownDescription": "https://circleci.com/docs/reusing-config#using-the-parameters-declaration\n\nA map of parameter keys." }, "resource_class": { "type": "string" }, "retention": { "type": "object", "properties": { "caches": { "type": "string", "pattern": "^([1-9]\|1[0-5])d$" } }, "additionalProperties": false }, "shell": { "oneOf": [ { "type": "string" }, { "type": "array", "items": { "type": "string" } } ] }, "steps": { "type": "array", "items": { "$ref": "#/definitions/step" }, "minItems": 1 }, "type": { "const": "build", "markdownDescription": "The job type. If not specified, defaults to build." }, "working_directory": { "type": "string", "markdownDescription": "In which directory to run the steps. (default: `~/project`. `project` is a literal string, not the name of the project.) You can also refer the directory with `$CIRCLE_WORKING_DIRECTORY` environment variable." } }, "additionalProperties": false, "markdownDescription": "Validation: The job must adhere to the structure of a build job." }, "else": false } } } } | — |
| in-process | codegen | massive/data-warehouse | — | — | — | — | — | — | — | — | — | — | — | pass | pass | — | — | FAIL: panic: Expected definition: `#/$defs/table_0000` $defs | pass | — |
| in-process | codegen | massive/k8s-deployment | — | — | — | — | — | — | — | — | — | — | — | FAIL: panic: Could not parse massive/k8s-deployment/schema.json: expected value at line 6 column 15 | pass | — | — | pass | pass | — |
| in-process | codegen | medium/package-json | — | — | — | — | — | — | — | — | — | — | — | FAIL: cannot load the fixture's resource documents | pass | — | — | FAIL: cannot load the fixture's resource documents | FAIL: cannot load the fixture's resource documents | — |
| in-process | codegen | small/prettierrc | — | — | — | — | — | — | — | — | — | — | — | pass | pass | — | — | pass | FAIL: codegen error: value does not conform to the given schema | — |
| in-process | schema compile | medium/package-json | — | — | pass | pass | — | — | — | pass | — | — | FAIL: cannot load the fixture's resource documents | — | pass | — | — | — | — | pass |
| in-process | validate (valid) | medium/package-json | — | — | pass | pass | — | — | — | pass | — | pass | FAIL: cannot load the fixture's resource documents | — | pass | — | — | — | — | pass |
| in-process | validate (invalid) | medium/package-json | — | — | pass | pass | — | — | — | pass | — | pass | FAIL: cannot load the fixture's resource documents | — | pass | — | — | — | — | pass |
| CLI | codegen | large/circleci | — | — | — | — | pass | — | pass | — | — | — | — | — | pass | — | FAIL: exit 1: Error: Internal error: We have additional properties but want to make a class. | — | FAIL: exit 101: not implemented: if/then/else schemas are not supported: { | — |
| CLI | codegen | medium/package-json | — | — | — | — | FAIL: cannot load the fixture's resource documents | — | FAIL: cannot load the fixture's resource documents | — | — | — | — | — | pass | — | FAIL: cannot load the fixture's resource documents | — | FAIL: cannot load the fixture's resource documents | — |
| CLI | codegen | small/prettierrc | — | — | — | — | pass | — | pass | — | — | — | — | — | pass | — | pass | — | FAIL: exit 1: value does not conform to the given schema | — |
| CLI | validate (valid) | generated/wide-object | FAIL: rejected valid instance (exit 2): ajv_validate: error: Maximum call stack size exceeded | pass | pass | — | — | pass | — | — | pass | — | — | — | pass | pass | — | — | — | — |
| CLI | validate (valid) | massive/k8s-deployment | FAIL: rejected valid instance (exit 2): ajv_validate: unsupported $schema http://json-schema.org/schema# | FAIL: rejected valid instance (exit 4): error: Could not resolve the metaschema of the schema | pass | — | — | pass | — | — | pass | — | — | — | pass | pass | — | — | — | — |
| CLI | validate (valid) | medium/package-json | pass | pass | pass | — | — | pass | — | — | FAIL: cannot load the fixture's resource documents | — | — | — | pass | pass | — | — | — | — |
| CLI | validate (invalid) | generated/wide-object | FAIL: not measured: valid instance was rejected | pass | pass | — | — | pass | — | — | pass | — | — | — | pass | pass | — | — | — | — |
| CLI | validate (invalid) | massive/k8s-deployment | FAIL: not measured: valid instance was rejected | FAIL: not measured: valid instance was rejected | pass | — | — | pass | — | — | pass | — | — | — | pass | pass | — | — | — | — |
| CLI | validate (invalid) | medium/package-json | pass | pass | pass | — | — | pass | — | — | FAIL: cannot load the fixture's resource documents | — | — | — | pass | pass | — | — | — | — |
