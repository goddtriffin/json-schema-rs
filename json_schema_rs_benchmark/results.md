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
| in-process | validate (invalid) | time | jsonschema-macros | 7 / 12 |
| in-process | validate (invalid) | peak heap | jsonschema | 5 / 12 |
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
| generated/deep-nesting | 831.239 ± 5.738 µs | 3.283 ± 0.028 ms | **442.027 ± 2.992 µs** | 10.982 ± 0.168 ms |
| generated/large-array | 69.256 ± 1.259 µs | 144.031 ± 1.096 µs | **7.657 ± 0.030 µs** | 1.099 ± 0.003 ms |
| generated/many-defs | 6.360 ± 0.058 ms | 22.087 ± 0.196 ms | **209.259 ± 0.601 µs** | 131.968 ± 1.400 ms |
| generated/wide-object | 9.336 ± 0.060 ms | 19.320 ± 0.125 ms | **8.905 ± 0.066 ms** | 161.431 ± 1.813 ms |
| large/api-catalog | 231.350 ± 4.636 µs | 493.010 ± 2.272 µs | **26.168 ± 0.092 µs** | 3.336 ± 0.014 ms |
| large/circleci | — | 7.351 ± 0.026 ms | **2.226 ± 0.015 ms** | — |
| massive/data-warehouse | **13.453 ± 0.120 ms** | 40.101 ± 0.483 ms | — | 294.010 ± 2.894 ms |
| massive/k8s-deployment | — | 17.141 ± 0.173 ms | 11.153 ± 0.106 ms | **1.929 ± 0.007 ms** |
| medium/package-json | — | 50.176 ± 1.121 ms | — | — |
| medium/user-profile | 105.018 ± 1.208 µs | 199.847 ± 1.151 µs | **35.582 ± 0.184 µs** | 1.534 ± 0.004 ms |
| small/config | 47.317 ± 1.512 µs | 69.154 ± 0.176 µs | **20.223 ± 0.098 µs** | 553.835 ± 7.445 µs |
| small/prettierrc | **269.063 ± 2.611 µs** | 931.555 ± 8.352 µs | 889.565 ± 2.820 µs | — |

### codegen — peak heap

| Fixture | jsonschema_code_generator | jsonschemars | schemafy | typify |
|---|---|---|---|---|
| generated/deep-nesting | **468.85 KiB** | 1.39 MiB | 680.02 KiB | 12.65 MiB |
| generated/large-array | 38.63 KiB | 81.24 KiB | **19.86 KiB** | 1.23 MiB |
| generated/many-defs | 5.97 MiB | 10.85 MiB | **25.63 KiB** | 133.38 MiB |
| generated/wide-object | **4.60 MiB** | 10.44 MiB | 7.73 MiB | 162.36 MiB |
| large/api-catalog | 157.72 KiB | 285.27 KiB | **31.92 KiB** | 3.64 MiB |
| large/circleci | — | 3.90 MiB | **2.43 MiB** | — |
| massive/data-warehouse | **9.03 MiB** | 16.82 MiB | — | 288.90 MiB |
| massive/k8s-deployment | — | 8.42 MiB | 10.36 MiB | **2.79 MiB** |
| medium/package-json | — | 32.87 MiB | — | — |
| medium/user-profile | 59.96 KiB | 116.96 KiB | **40.33 KiB** | 1.71 MiB |
| small/config | **20.63 KiB** | 40.64 KiB | 29.54 KiB | 639.58 KiB |
| small/prettierrc | **197.95 KiB** | 528.44 KiB | 995.53 KiB | — |

### schema compile — time

| Fixture | boon | corvus-json-schema | jsonschema | jsonschema-valid | jsonschemars | valico |
|---|---|---|---|---|---|---|
| generated/deep-nesting | 1.090 ± 0.002 ms | 264.941 ± 2.408 µs | 220.891 ± 0.678 µs | **68.038 ± 0.111 µs** | 107.239 ± 0.313 µs | 1.639 ± 0.009 ms |
| generated/large-array | 47.944 ± 0.134 µs | 27.341 ± 0.071 µs | 21.813 ± 0.039 µs | **4.446 ± 0.021 µs** | 11.399 ± 0.035 µs | 37.621 ± 0.195 µs |
| generated/many-defs | 15.299 ± 0.172 ms | 3.335 ± 0.042 ms | 3.267 ± 0.024 ms | **676.696 ± 2.643 µs** | 1.736 ± 0.012 ms | 4.687 ± 0.045 ms |
| generated/wide-object | 25.662 ± 0.355 ms | 5.786 ± 0.018 ms | 2.788 ± 0.036 ms | **757.591 ± 4.095 µs** | 1.402 ± 0.004 ms | 3.620 ± 0.035 ms |
| large/api-catalog | 191.129 ± 0.500 µs | 95.808 ± 0.227 µs | 109.989 ± 0.469 µs | **15.636 ± 0.092 µs** | 46.519 ± 0.073 µs | 144.141 ± 0.297 µs |
| large/circleci | 3.082 ± 0.023 ms | 1.021 ± 0.006 ms | 1.100 ± 0.006 ms | **228.626 ± 0.643 µs** | 545.828 ± 2.016 µs | 4.370 ± 0.051 ms |
| massive/data-warehouse | 51.856 ± 0.302 ms | 6.911 ± 0.029 ms | 7.317 ± 0.106 ms | **1.266 ± 0.007 ms** | 2.843 ± 0.017 ms | 11.310 ± 0.278 ms |
| massive/k8s-deployment | 7.609 ± 0.031 ms | 2.347 ± 0.020 ms | 1.641 ± 0.012 ms | **541.108 ± 8.018 µs** | 787.192 ± 18.076 µs | 8.309 ± 0.131 ms |
| medium/package-json | 76.928 ± 0.553 ms | 9.828 ± 0.081 ms | 18.070 ± 0.388 ms | — | **6.723 ± 0.073 ms** | 20.170 ± 0.167 ms |
| medium/user-profile | 73.027 ± 1.080 µs | 36.922 ± 0.144 µs | 42.931 ± 0.226 µs | **6.428 ± 0.055 µs** | 17.791 ± 0.863 µs | 49.101 ± 0.354 µs |
| small/config | 28.724 ± 0.236 µs | 11.050 ± 0.040 µs | 20.401 ± 0.105 µs | **2.599 ± 0.025 µs** | 8.193 ± 0.071 µs | 21.076 ± 0.562 µs |
| small/prettierrc | 207.245 ± 5.825 µs | 159.646 ± 0.466 µs | 114.215 ± 1.861 µs | **24.904 ± 0.194 µs** | 61.663 ± 1.413 µs | 229.445 ± 0.253 µs |

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
| small/config | 38.35 KiB | 21.47 KiB | 24.08 KiB | **5.51 KiB** | 10.86 KiB | 44.02 KiB |
| small/prettierrc | 476.85 KiB | 310.60 KiB | 261.04 KiB | **80.78 KiB** | 115.58 KiB | 708.31 KiB |

### validate (valid) — time

| Fixture | boon | corvus-json-schema | jsonschema | jsonschema-macros | jsonschema-valid | jsonschemars | valico |
|---|---|---|---|---|---|---|---|
| generated/deep-nesting | 6.921 ± 0.015 µs | 27.749 ± 0.089 µs | 2.375 ± 0.039 µs | **570.000 ± 14.927 ns** | 18.703 ± 0.070 µs | 3.022 ± 0.031 µs | 556.863 ± 1.004 µs |
| generated/large-array | 18.419 ± 0.291 ms | 71.930 ± 0.539 ms | 7.650 ± 0.108 ms | **2.523 ± 0.046 ms** | 83.123 ± 0.585 ms | 8.986 ± 0.185 ms | 263.897 ± 3.327 ms |
| generated/many-defs | 1.735 ± 0.011 ms | 4.640 ± 0.060 ms | 620.951 ± 2.618 µs | **109.470 ± 2.053 µs** | 315.347 ± 2.399 ms | 766.920 ± 9.577 µs | 39.015 ± 0.311 ms |
| generated/wide-object | 170.925 ± 0.485 µs | 431.608 ± 6.392 µs | 111.203 ± 1.427 µs | 151.483 ± 1.887 µs | 472.084 ± 2.167 µs | **54.623 ± 0.254 µs** | 1.242 ± 0.006 ms |
| large/api-catalog | 572.558 ± 4.928 ns | 2.200 ± 0.032 µs | 241.876 ± 1.425 ns | **96.513 ± 3.994 ns** | 27.506 ± 0.061 µs | 434.807 ± 7.375 ns | 6.472 ± 0.027 µs |
| large/circleci | 2.226 ± 0.074 µs | 12.221 ± 0.128 µs | 467.729 ± 2.926 ns | **216.010 ± 7.783 ns** | 167.836 ± 10.948 µs | 1.001 ± 0.005 µs | 20.187 ± 0.185 µs |
| massive/data-warehouse | 71.861 ± 1.092 µs | 55.303 ± 0.177 ms | 24.744 ± 0.269 µs | **7.724 ± 0.155 µs** | 26.590 ± 0.343 ms | 47.936 ± 0.731 µs | 189.639 ± 4.772 ms |
| massive/k8s-deployment | 522.976 ± 3.777 ns | 2.582 ± 0.037 µs | 216.792 ± 1.518 ns | **55.620 ± 1.267 ns** | 3.067 ± 0.018 µs | 506.201 ± 5.210 ns | 7.832 ± 0.038 µs |
| medium/package-json | 558.125 ± 14.526 ns | 2.801 ± 0.068 µs | **245.555 ± 7.136 ns** | 762.270 ± 13.630 ns | — | 485.815 ± 6.402 ns | 5.664 ± 0.027 µs |
| medium/user-profile | 854.570 ± 11.138 ns | 3.561 ± 0.074 µs | 393.893 ± 11.397 ns | **178.563 ± 4.455 ns** | 13.085 ± 0.106 µs | 511.251 ± 14.806 ns | 8.628 ± 0.064 µs |
| small/config | 223.393 ± 0.833 ns | 907.366 ± 10.549 ns | 114.853 ± 2.018 ns | **64.072 ± 1.271 ns** | 9.163 ± 0.127 µs | 144.289 ± 0.735 ns | 1.872 ± 0.020 µs |
| small/prettierrc | 730.757 ± 1.244 ns | 5.058 ± 0.133 µs | 176.998 ± 3.197 ns | **53.353 ± 1.291 ns** | 8.504 ± 0.122 µs | 278.501 ± 5.583 ns | 7.400 ± 0.076 µs |

### validate (valid) — peak heap

| Fixture | boon | corvus-json-schema | jsonschema | jsonschema-macros | jsonschema-valid | jsonschemars | valico |
|---|---|---|---|---|---|---|---|
| generated/deep-nesting | 2.25 KiB | 23.40 KiB | 0.03 KiB | 0.03 KiB | 13.29 KiB | **0.00 KiB** | 37.75 KiB |
| generated/large-array | 0.19 KiB | 1.49 KiB | 0.03 KiB | 0.03 KiB | 2.05 KiB | **0.00 KiB** | 39.97 MiB |
| generated/many-defs | 0.56 KiB | 2.29 KiB | 0.03 KiB | 0.03 KiB | 10.12 KiB | **0.00 KiB** | 3.34 MiB |
| generated/wide-object | 0.19 KiB | 1.19 KiB | 0.03 KiB | 0.03 KiB | 0.76 KiB | **0.00 KiB** | 236.68 KiB |
| large/api-catalog | 0.19 KiB | 1.48 KiB | 0.03 KiB | 0.03 KiB | 10.41 KiB | **0.00 KiB** | 1.44 KiB |
| large/circleci | 2.58 KiB | 3.26 KiB | 0.03 KiB | 0.03 KiB | 328.25 KiB | 0.22 KiB | 3.41 KiB |
| massive/data-warehouse | 0.19 KiB | 1.48 KiB | 0.03 KiB | 0.03 KiB | 12.55 KiB | **0.00 KiB** | 155.85 KiB |
| massive/k8s-deployment | 0.19 KiB | 1.59 KiB | 0.03 KiB | 0.03 KiB | 2.07 KiB | **0.00 KiB** | 1.62 KiB |
| medium/package-json | 0.19 KiB | 1.23 KiB | 0.03 KiB | 0.03 KiB | — | **0.00 KiB** | 1.20 KiB |
| medium/user-profile | 0.19 KiB | 1.23 KiB | 0.03 KiB | 0.03 KiB | 8.74 KiB | **0.00 KiB** | 1.88 KiB |
| small/config | 0.19 KiB | 1.18 KiB | 0.03 KiB | 0.03 KiB | 10.18 KiB | **0.00 KiB** | 0.60 KiB |
| small/prettierrc | 0.77 KiB | 1.49 KiB | 0.03 KiB | 0.03 KiB | 2.05 KiB | **0.00 KiB** | 1.71 KiB |

### validate (invalid) — time

| Fixture | boon | corvus-json-schema | jsonschema | jsonschema-macros | jsonschema-valid | jsonschemars | valico |
|---|---|---|---|---|---|---|---|
| generated/deep-nesting | 7.806 ± 0.024 µs | 33.898 ± 0.742 µs | **2.801 ± 0.033 µs** | 14.381 ± 0.368 µs | 23.247 ± 0.247 µs | 5.285 ± 0.111 µs | 557.424 ± 4.123 µs |
| generated/large-array | 18.318 ± 0.208 ms | 75.760 ± 3.095 ms | 7.334 ± 0.151 ms | **5.157 ± 0.174 ms** | 84.270 ± 1.242 ms | 8.573 ± 0.272 ms | 259.594 ± 6.763 ms |
| generated/many-defs | 1.754 ± 0.059 ms | 4.738 ± 0.192 ms | 623.056 ± 16.543 µs | **137.351 ± 2.168 µs** | 316.879 ± 3.474 ms | 748.372 ± 5.076 µs | 43.249 ± 1.524 ms |
| generated/wide-object | 173.095 ± 1.339 µs | 427.530 ± 13.738 µs | 114.436 ± 2.680 µs | 159.020 ± 7.699 µs | 479.876 ± 5.120 µs | **54.536 ± 0.775 µs** | 1.249 ± 0.012 ms |
| large/api-catalog | 653.992 ± 3.746 ns | 2.432 ± 0.053 µs | 399.465 ± 3.053 ns | **317.700 ± 8.155 ns** | 20.599 ± 0.261 µs | 1.056 ± 0.020 µs | 4.324 ± 0.057 µs |
| large/circleci | **130.324 ± 0.927 ns** | 577.808 ± 22.041 ns | 137.165 ± 1.459 ns | 151.516 ± 2.902 ns | 548.599 ± 4.964 ns | 275.607 ± 7.011 ns | 458.382 ± 5.138 ns |
| massive/data-warehouse | 181.592 ± 1.387 µs | 382.203 ± 8.517 µs | 141.202 ± 2.298 µs | **107.127 ± 0.730 µs** | 43.817 ± 1.194 ms | 271.347 ± 3.640 µs | 915.101 ± 126.754 µs † |
| massive/k8s-deployment | 188.962 ± 4.866 ns | 715.755 ± 24.820 ns | 147.707 ± 7.808 ns | **129.588 ± 5.281 ns** | 241.156 ± 5.635 µs | 240.008 ± 0.838 ns | 899.039 ± 9.516 ns |
| medium/package-json | 176.111 ± 0.570 ns | 798.710 ± 33.020 ns | **158.901 ± 1.954 ns** | 394.426 ± 13.173 ns | — | 251.795 ± 2.989 ns | 841.246 ± 11.877 ns |
| medium/user-profile | 394.889 ± 7.314 ns | 1.774 ± 0.024 µs | 334.039 ± 9.202 ns | **292.710 ± 7.187 ns** | 8.588 ± 0.043 µs | 573.195 ± 24.384 ns | 2.560 ± 0.010 µs |
| small/config | 311.384 ± 1.576 ns | 1.492 ± 0.035 µs | 271.398 ± 2.728 ns | **228.432 ± 2.163 ns** | 10.102 ± 0.058 µs | 566.064 ± 3.920 ns | 1.754 ± 0.008 µs |
| small/prettierrc | **544.511 ± 2.844 ns** | 2.548 ± 0.076 µs | 890.551 ± 13.798 ns | 801.130 ± 14.452 ns | 5.144 ± 0.120 µs | 1.624 ± 0.011 µs | 2.002 ± 0.012 µs |

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
| medium/user-profile | 1.30 KiB | 2.52 KiB | 0.73 KiB | 0.82 KiB | 9.73 KiB | 1.48 KiB | **0.67 KiB** |
| small/config | 1.30 KiB | 2.51 KiB | 0.71 KiB | 0.80 KiB | 11.52 KiB | 1.60 KiB | **0.69 KiB** |
| small/prettierrc | 2.38 KiB | 2.07 KiB | 2.69 KiB | 2.76 KiB | 4.50 KiB | 3.27 KiB | **1.74 KiB** |

## CLI (end to end)

### codegen — time

| Fixture | datamodel-code-generator | json-schema-to-typescript | jsonschemars | quicktype | typify |
|---|---|---|---|---|---|
| *startup* | 211.663 ± 2.191 ms | 154.036 ± 0.858 ms | 2.378 ± 0.273 ms † | 264.811 ± 1.822 ms | 84.527 ± 2.288 ms |
| generated/deep-nesting | 227.571 ± 2.015 ms | 168.843 ± 1.241 ms | **6.073 ± 0.237 ms** | 289.417 ± 3.869 ms | 108.078 ± 0.702 ms |
| generated/large-array | 222.309 ± 2.189 ms | 160.526 ± 1.231 ms | **2.204 ± 0.051 ms** | 274.131 ± 1.868 ms | 88.460 ± 1.834 ms |
| generated/many-defs | 460.400 ± 8.773 ms | 240.178 ± 4.959 ms | **27.998 ± 0.174 ms** | 448.524 ± 9.870 ms | 446.654 ± 9.547 ms |
| generated/wide-object | 600.407 ± 2.585 ms | 283.643 ± 0.941 ms | **24.713 ± 0.383 ms** | 464.647 ± 3.173 ms | 504.724 ± 3.173 ms |
| large/api-catalog | 227.618 ± 5.344 ms | 163.381 ± 2.959 ms | **2.743 ± 0.053 ms** | 284.316 ± 1.656 ms | 93.778 ± 0.622 ms |
| large/circleci | 294.693 ± 1.752 ms | 199.058 ± 0.904 ms | **11.008 ± 0.137 ms** | — | — |
| massive/data-warehouse | 1.024 ± 0.004 s | 320.493 ± 2.935 ms | **43.391 ± 0.179 ms** | 598.060 ± 3.334 ms | 785.134 ± 4.343 ms |
| massive/k8s-deployment | 425.797 ± 4.993 ms | 263.494 ± 1.777 ms | **21.439 ± 0.163 ms** | 412.021 ± 11.491 ms | 86.249 ± 1.040 ms |
| medium/package-json | — | — | 97.916 ± 0.389 ms | — | — |
| medium/user-profile | 220.608 ± 0.587 ms | 159.541 ± 0.988 ms | **2.268 ± 0.064 ms** | 275.427 ± 1.446 ms | 90.027 ± 1.143 ms |
| small/config | 213.784 ± 0.979 ms | 156.614 ± 1.428 ms | **1.979 ± 0.066 ms** | 269.656 ± 5.104 ms | 87.008 ± 1.055 ms |
| small/prettierrc | 254.434 ± 0.847 ms | 168.641 ± 1.349 ms | **3.634 ± 0.130 ms** | 292.570 ± 5.156 ms | — |

### codegen — peak RSS

| Fixture | datamodel-code-generator | json-schema-to-typescript | jsonschemars | quicktype | typify |
|---|---|---|---|---|---|
| *startup* | 58.98 MiB | 73.33 ± 0.04 MiB | 3.72 ± 0.01 MiB | 127.25 ± 0.41 MiB | 31.67 MiB |
| generated/deep-nesting | 60.98 ± 0.05 MiB | 78.08 ± 0.02 MiB | **7.52 ± 0.08 MiB** | 151.22 ± 4.33 MiB | 37.86 ± 0.02 MiB |
| generated/large-array | 60.00 ± 0.01 MiB | 75.56 ± 0.19 MiB | **4.58 MiB** | 130.70 ± 0.33 MiB | 33.19 ± 0.02 MiB |
| generated/many-defs | 82.91 MiB | 121.91 ± 0.60 MiB | **22.38 ± 0.28 MiB** | 218.58 ± 1.44 MiB | 97.94 ± 0.29 MiB |
| generated/wide-object | 97.48 ± 0.28 MiB | 167.05 MiB | **22.70 ± 0.01 MiB** | 216.91 ± 7.35 MiB | 114.50 ± 0.16 MiB |
| large/api-catalog | 62.94 ± 0.06 MiB | 76.02 ± 0.15 MiB | **5.20 ± 0.02 MiB** | 142.11 MiB | 34.59 ± 0.04 MiB |
| large/circleci | 69.73 ± 0.06 MiB | 91.47 MiB | **10.72 ± 0.07 MiB** | — | — |
| massive/data-warehouse | 175.56 ± 0.01 MiB | 178.64 ± 0.95 MiB | **32.39 ± 0.52 MiB** | 233.22 ± 0.10 MiB | 176.22 ± 0.30 MiB |
| massive/k8s-deployment | 87.14 MiB | 129.39 ± 0.26 MiB | **17.36 ± 0.00 MiB** | 213.38 MiB | 31.66 ± 0.01 MiB |
| medium/package-json | — | — | 40.92 MiB | — | — |
| medium/user-profile | 60.23 MiB | 76.45 MiB | **4.78 ± 0.04 MiB** | 132.58 ± 0.23 MiB | 33.44 ± 0.03 MiB |
| small/config | 58.67 MiB | 74.44 ± 0.02 MiB | **4.36 ± 0.02 MiB** | 127.94 MiB | 32.78 ± 0.01 MiB |
| small/prettierrc | 67.11 MiB | 77.94 ± 0.50 MiB | **5.59 MiB** | 149.73 MiB | — |

### validate (valid) — time

| Fixture | ajv | blaze | boon | fastjsonschema | jsonschema-cli | jsonschemars | python-jsonschema |
|---|---|---|---|---|---|---|---|
| *startup* | 113.947 ± 1.572 ms | 4.138 ± 0.444 ms † | 2.956 ± 0.125 ms | 25.155 ± 0.351 ms | 3.696 ± 0.161 ms | 1.638 ± 0.063 ms | 59.785 ± 0.953 ms |
| generated/deep-nesting | 143.939 ± 5.489 ms | 14.238 ± 0.306 ms | 4.217 ± 0.067 ms | 95.291 ± 0.364 ms | 4.234 ± 0.220 ms | **1.858 ± 0.046 ms** | 75.448 ± 1.162 ms |
| generated/large-array | 159.605 ± 1.794 ms | 97.852 ± 0.918 ms | 58.477 ± 0.679 ms | 149.559 ± 4.428 ms | 44.917 ± 0.502 ms | 44.812 ± 0.415 ms | 1.386 ± 0.015 s |
| generated/many-defs | 239.892 ± 1.928 ms | 115.874 ± 0.671 ms | 23.186 ± 0.399 ms | 384.064 ± 1.281 ms | 11.676 ± 0.186 ms | **7.510 ± 0.172 ms** | 413.147 ± 2.273 ms |
| generated/wide-object | — | 58.756 ± 0.743 ms | 27.870 ± 0.283 ms | 312.325 ± 21.188 ms | 8.135 ± 0.172 ms | **3.846 ± 0.040 ms** | 294.846 ± 2.097 ms |
| large/api-catalog | 124.848 ± 0.929 ms | 4.891 ± 0.077 ms | 3.150 ± 0.034 ms | 36.427 ± 0.205 ms | 4.135 ± 0.420 ms † | **1.807 ± 0.039 ms** | 65.878 ± 0.556 ms |
| large/circleci | 164.634 ± 1.767 ms | 28.268 ± 0.296 ms | 6.866 ± 0.164 ms | 323.160 ± 1.394 ms | 5.945 ± 0.074 ms | **2.563 ± 0.029 ms** | 74.111 ± 0.263 ms |
| massive/data-warehouse | 553.020 ± 2.090 ms | 238.045 ± 1.377 ms | 49.603 ± 0.758 ms | 1.214 ± 0.003 s | 12.093 ± 0.122 ms | **5.409 ± 0.086 ms** | 458.378 ± 4.929 ms |
| massive/k8s-deployment | — | — | 11.987 ± 0.120 ms | 357.875 ± 1.663 ms | 343.697 ± 168.462 ms † | **2.970 ± 0.026 ms** | 219.226 ± 3.281 ms |
| medium/package-json | 857.597 ± 49.898 ms | 361.330 ± 3.894 ms | 75.035 ± 0.496 ms | 868.712 ± 3.563 ms | — | **7.721 ± 0.165 ms** | 69.134 ± 0.434 ms |
| medium/user-profile | 122.759 ± 4.296 ms | 3.876 ± 0.109 ms | 2.970 ± 0.051 ms | 28.783 ± 0.211 ms | 3.860 ± 0.111 ms | **1.725 ± 0.034 ms** | 62.402 ± 0.633 ms |
| small/config | 120.874 ± 0.865 ms | 3.357 ± 0.093 ms | 2.903 ± 0.040 ms | 26.193 ± 0.626 ms | 3.769 ± 0.170 ms | **1.646 ± 0.027 ms** | 60.263 ± 0.865 ms |
| small/prettierrc | 123.985 ± 0.786 ms | 5.927 ± 0.159 ms | 3.131 ± 0.031 ms | 38.083 ± 0.281 ms | 3.952 ± 0.565 ms † | **1.811 ± 0.040 ms** | 60.913 ± 0.346 ms |

### validate (valid) — peak RSS

| Fixture | ajv | blaze | boon | fastjsonschema | jsonschema-cli | jsonschemars | python-jsonschema |
|---|---|---|---|---|---|---|---|
| *startup* | 53.25 ± 0.01 MiB | 8.50 MiB | 5.66 ± 0.01 MiB | 17.17 ± 0.03 MiB | 8.44 MiB | 3.48 MiB | 24.67 ± 0.01 MiB |
| generated/deep-nesting | 62.53 ± 0.18 MiB | 18.09 ± 0.00 MiB | 7.02 ± 0.03 MiB | 87.36 ± 0.01 MiB | 10.03 MiB | **4.14 MiB** | 25.36 ± 0.15 MiB |
| generated/large-array | 84.19 ± 0.03 MiB | 300.80 MiB | **56.08 MiB** | 60.20 ± 0.02 MiB | 64.95 MiB | 59.66 ± 0.00 MiB | 65.64 ± 0.03 MiB |
| generated/many-defs | 80.84 ± 0.16 MiB | 84.83 MiB | 19.17 ± 0.02 MiB | 327.09 MiB | 21.62 MiB | **13.12 ± 0.03 MiB** | 28.20 ± 0.03 MiB |
| generated/wide-object | — | 26.86 MiB | 15.25 ± 0.01 MiB | 283.50 ± 1.92 MiB | 15.52 MiB | **7.34 MiB** | 26.28 ± 0.01 MiB |
| large/api-catalog | 55.61 ± 0.06 MiB | 9.56 ± 0.01 MiB | 6.00 ± 0.01 MiB | 27.44 MiB | 10.56 ± 0.01 MiB | **3.94 ± 0.00 MiB** | 24.70 ± 0.06 MiB |
| large/circleci | 75.33 ± 0.04 MiB | 19.53 ± 0.01 MiB | 9.23 ± 0.01 MiB | 270.14 ± 0.04 MiB | 13.31 ± 0.02 MiB | **5.48 ± 0.01 MiB** | 25.16 ± 0.01 MiB |
| massive/data-warehouse | 149.05 ± 0.07 MiB | 297.67 ± 0.01 MiB | 22.19 ± 0.01 MiB | 973.69 MiB | 21.19 MiB | **9.38 ± 0.01 MiB** | 27.33 ± 0.03 MiB |
| massive/k8s-deployment | — | — | 11.84 MiB | 313.28 ± 0.02 MiB | 20.38 MiB | **6.06 ± 0.00 MiB** | 26.31 ± 0.02 MiB |
| medium/package-json | 363.05 MiB | 68.64 MiB | 27.78 ± 0.00 MiB | 660.69 MiB | — | **11.53 ± 0.05 MiB** | 28.27 MiB |
| medium/user-profile | 54.72 MiB | 9.22 MiB | 5.86 ± 0.02 MiB | 20.47 ± 0.02 MiB | 10.33 ± 0.02 MiB | **3.80 MiB** | 24.66 MiB |
| small/config | 54.38 MiB | 8.91 MiB | 5.83 MiB | 18.02 ± 0.01 MiB | 10.17 ± 0.03 MiB | **3.69 MiB** | 24.78 MiB |
| small/prettierrc | 58.00 MiB | 10.11 ± 0.01 MiB | 5.91 ± 0.02 MiB | 30.06 ± 0.02 MiB | 9.22 MiB | **3.92 ± 0.01 MiB** | 24.66 ± 0.02 MiB |

### validate (invalid) — time

| Fixture | ajv | blaze | boon | fastjsonschema | jsonschema-cli | jsonschemars | python-jsonschema |
|---|---|---|---|---|---|---|---|
| *startup* | 113.664 ± 1.349 ms | 3.261 ± 0.126 ms | 2.826 ± 0.093 ms | 25.587 ± 0.568 ms | 3.618 ± 0.069 ms | 1.710 ± 0.091 ms | 59.489 ± 0.500 ms |
| generated/deep-nesting | 137.829 ± 3.458 ms | 35.440 ± 0.233 ms | 4.200 ± 0.103 ms | 95.566 ± 0.676 ms | 4.244 ± 0.061 ms | **1.885 ± 0.058 ms** | 76.493 ± 2.131 ms |
| generated/large-array | 158.833 ± 1.545 ms | 114.177 ± 0.571 ms | 58.657 ± 0.283 ms | 146.030 ± 0.874 ms | 45.050 ± 0.598 ms | 44.805 ± 0.462 ms | 1.404 ± 0.017 s |
| generated/many-defs | 240.186 ± 3.079 ms | 119.603 ± 0.747 ms | 23.007 ± 0.372 ms | 384.216 ± 2.047 ms | 11.533 ± 0.070 ms | **7.542 ± 0.119 ms** | 417.172 ± 9.592 ms |
| generated/wide-object | — | 58.170 ± 0.575 ms | 27.727 ± 0.172 ms | 304.530 ± 1.710 ms | 8.192 ± 0.069 ms | **3.899 ± 0.109 ms** | 294.957 ± 1.577 ms |
| large/api-catalog | 125.008 ± 0.807 ms | 5.273 ± 0.354 ms | 3.160 ± 0.182 ms | 36.671 ± 0.355 ms | 4.101 ± 0.085 ms | **1.821 ± 0.051 ms** | 65.699 ± 0.929 ms |
| large/circleci | 162.649 ± 1.446 ms | 28.431 ± 0.161 ms | 6.797 ± 0.061 ms | 323.265 ± 1.194 ms | 5.841 ± 0.070 ms | **2.584 ± 0.119 ms** | 73.933 ± 0.458 ms |
| massive/data-warehouse | 493.413 ± 1.954 ms | 303.232 ± 0.768 ms | 50.727 ± 0.185 ms | 1.239 ± 0.045 s | 12.171 ± 0.298 ms | **5.660 ± 0.200 ms** | 462.182 ± 3.071 ms |
| massive/k8s-deployment | — | — | 11.876 ± 0.042 ms | 358.609 ± 1.971 ms | 262.601 ± 162.254 ms † | **2.943 ± 0.080 ms** | 219.766 ± 0.878 ms |
| medium/package-json | 843.290 ± 8.624 ms | 365.020 ± 7.299 ms | 74.946 ± 0.412 ms | 867.803 ± 3.525 ms | — | **7.678 ± 0.147 ms** | 69.508 ± 0.272 ms |
| medium/user-profile | 122.630 ± 0.817 ms | 3.997 ± 0.109 ms | 3.089 ± 0.081 ms | 28.750 ± 0.223 ms | 3.861 ± 0.196 ms | **1.692 ± 0.058 ms** | 62.203 ± 0.291 ms |
| small/config | 120.520 ± 0.943 ms | 3.474 ± 0.069 ms | 2.978 ± 0.062 ms | 27.519 ± 2.637 ms | 3.845 ± 0.229 ms | **1.663 ± 0.066 ms** | 60.213 ± 0.273 ms |
| small/prettierrc | 123.995 ± 0.953 ms | 6.385 ± 0.345 ms | 3.185 ± 0.161 ms | 38.125 ± 0.188 ms | 3.880 ± 0.074 ms | **1.867 ± 0.121 ms** | 60.806 ± 0.500 ms |

### validate (invalid) — peak RSS

| Fixture | ajv | blaze | boon | fastjsonschema | jsonschema-cli | jsonschemars | python-jsonschema |
|---|---|---|---|---|---|---|---|
| *startup* | 53.38 ± 0.05 MiB | 8.67 MiB | 5.67 ± 0.01 MiB | 17.19 ± 0.02 MiB | 8.56 ± 0.01 MiB | 3.53 MiB | 24.91 MiB |
| generated/deep-nesting | 63.00 ± 0.18 MiB | 18.27 MiB | 7.00 ± 0.02 MiB | 87.36 MiB | 10.16 MiB | **4.23 MiB** | 24.97 MiB |
| generated/large-array | 84.27 ± 0.96 MiB | 326.53 MiB | **55.92 MiB** | 60.34 ± 0.07 MiB | 65.02 MiB | 59.73 ± 0.01 MiB | 65.66 MiB |
| generated/many-defs | 80.58 ± 0.16 MiB | 85.44 MiB | 19.20 ± 0.02 MiB | 327.05 ± 0.03 MiB | 21.67 ± 0.01 MiB | **13.17 ± 0.05 MiB** | 28.38 ± 0.07 MiB |
| generated/wide-object | — | 27.56 MiB | 15.25 MiB | 283.50 ± 2.26 MiB | 15.59 ± 0.01 MiB | **7.41 MiB** | 26.23 ± 0.06 MiB |
| large/api-catalog | 55.61 ± 0.09 MiB | 9.70 MiB | 6.00 ± 0.01 MiB | 27.44 ± 0.03 MiB | 10.70 ± 0.01 MiB | **4.00 ± 0.01 MiB** | 24.67 MiB |
| large/circleci | 75.19 MiB | 19.47 ± 0.01 MiB | 9.19 ± 0.01 MiB | 270.14 ± 0.07 MiB | 13.28 ± 0.02 MiB | **5.56 ± 0.01 MiB** | 25.23 ± 0.02 MiB |
| massive/data-warehouse | 133.70 ± 0.35 MiB | 68.66 MiB | 22.89 ± 0.01 MiB | 971.52 ± 0.02 MiB | 21.78 ± 0.23 MiB | **10.06 ± 0.24 MiB** | 28.09 ± 0.04 MiB |
| massive/k8s-deployment | — | — | 11.81 ± 0.01 MiB | 318.69 ± 2.31 MiB | 20.31 ± 0.10 MiB | **6.08 ± 0.04 MiB** | 26.47 MiB |
| medium/package-json | 361.80 ± 0.13 MiB | 68.72 MiB | 27.77 ± 0.01 MiB | 661.69 MiB | — | **11.66 MiB** | 28.16 ± 0.05 MiB |
| medium/user-profile | 54.73 ± 0.03 MiB | 9.31 MiB | 5.89 MiB | 20.48 ± 0.01 MiB | 10.50 ± 0.01 MiB | **3.83 MiB** | 24.70 ± 0.01 MiB |
| small/config | 54.61 ± 0.10 MiB | 9.06 MiB | 5.86 MiB | 18.02 MiB | 10.34 ± 0.01 MiB | **3.78 MiB** | 24.67 ± 0.05 MiB |
| small/prettierrc | 58.06 MiB | 10.20 MiB | 5.98 ± 0.01 MiB | 30.05 ± 0.02 MiB | 9.36 ± 0.02 MiB | **3.97 MiB** | 24.75 ± 0.04 MiB |

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
