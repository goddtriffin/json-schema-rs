# Benchmark results

Two tiers, lower is better everywhere. **In-process**: every tool is linked as a Rust library and called on in-memory inputs (corvus-json-schema and jsonschema-macros each from a build of its own, so no tool's dependency features reach another's); time is criterion's median ± standard deviation per call, and peak heap is the extra heap the operation allocates, recorded by a counting allocator. jsonschema-macros compiles each schema into a validator when the benchmark is built, so it has no schema compile cells. Every Rust tool, ours and the competitors' alike (in-process and CLI), is built with the same profile, given in the Tools table's rustc row. **CLI**: every tool's binary runs end to end (process startup, file I/O, and the operation), timed by hyperfine as median ± standard deviation over repeated runs after warmup; peak RSS is the process's peak resident set size. Tools run on different runtimes (see Tools), and process start, runtime boot and module loading are part of every CLI run: each CLI table's *startup* row is the tool on a trivial input (a draft-07 `{"type": "object"}` schema), so that fixed cost can be told apart from the work, and it never counts toward a winner. A cell is measured only when the tool is correct on it (see Compatibility). A cell's best tool wins outright (**bold**) only when at least two tools were measured and its lead over the runner-up exceeds both 3% of its median and the sum of their standard deviations; otherwise the cell is a tie or uncontested and nothing is bolded. `†` marks a measurement whose standard deviation exceeds 10% of its median.

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
| rustc | 1.99.1 | compiles every Rust tool (one profile: codegen-units = 1, lto = "thin") |
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
| generated/deep-nesting | 687.919 ± 2.061 µs | 2.861 ± 0.008 ms | **381.348 ± 0.943 µs** | 8.458 ± 0.094 ms |
| generated/large-array | 59.050 ± 0.177 µs | 125.362 ± 0.267 µs | **6.521 ± 0.014 µs** | 891.545 ± 1.423 µs |
| generated/many-defs | 5.326 ± 0.013 ms | 19.234 ± 0.196 ms | **196.035 ± 0.717 µs** | 106.672 ± 0.676 ms |
| generated/wide-object | 7.917 ± 0.044 ms | 17.081 ± 0.057 ms | **7.627 ± 0.014 ms** | 129.221 ± 0.487 ms |
| large/api-catalog | 186.374 ± 0.671 µs | 433.449 ± 0.612 µs | **21.864 ± 0.052 µs** | 2.700 ± 0.007 ms |
| large/circleci | — | 6.507 ± 0.052 ms | **1.853 ± 0.004 ms** | — |
| massive/data-warehouse | **10.774 ± 0.032 ms** | 34.782 ± 0.314 ms | — | 236.623 ± 1.237 ms |
| massive/k8s-deployment | — | 14.825 ± 0.044 ms | 9.148 ± 0.028 ms | **1.714 ± 0.004 ms** |
| medium/package-json | — | 42.306 ± 0.717 ms | — | — |
| medium/user-profile | 80.232 ± 0.427 µs | 173.041 ± 0.462 µs | **29.241 ± 0.110 µs** | 1.230 ± 0.002 ms |
| small/config | 35.047 ± 0.212 µs | 58.976 ± 0.121 µs | **16.540 ± 0.042 µs** | 440.092 ± 0.681 µs |
| small/prettierrc | **212.621 ± 3.119 µs** | 817.385 ± 1.169 µs | 747.757 ± 3.086 µs | — |

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
| generated/deep-nesting | 994.654 ± 3.522 µs | 244.298 ± 2.301 µs | 187.008 ± 0.336 µs | **61.648 ± 0.200 µs** | 96.972 ± 0.264 µs | 1.347 ± 0.007 ms |
| generated/large-array | 42.572 ± 0.083 µs | 25.526 ± 0.274 µs | 18.325 ± 0.053 µs | **3.931 ± 0.042 µs** | 10.146 ± 0.024 µs | 31.890 ± 0.071 µs |
| generated/many-defs | 13.406 ± 0.034 ms | 3.093 ± 0.008 ms | 2.735 ± 0.013 ms | **588.146 ± 1.398 µs** | 1.564 ± 0.004 ms | 3.889 ± 0.020 ms |
| generated/wide-object | 23.012 ± 0.075 ms | 5.604 ± 0.021 ms | 2.250 ± 0.006 ms | **677.091 ± 4.790 µs** | 1.287 ± 0.003 ms | 3.069 ± 0.008 ms |
| large/api-catalog | 169.246 ± 0.437 µs | 88.924 ± 0.100 µs | 94.836 ± 0.655 µs | **13.832 ± 0.099 µs** | 40.963 ± 0.146 µs | 120.418 ± 0.131 µs |
| large/circleci | 2.784 ± 0.037 ms | 969.895 ± 1.615 µs | 937.728 ± 2.282 µs | **200.795 ± 0.412 µs** | 486.494 ± 1.662 µs | 3.736 ± 0.010 ms |
| massive/data-warehouse | 43.498 ± 0.105 ms | 6.536 ± 0.014 ms | 5.771 ± 0.046 ms | **1.109 ± 0.004 ms** | 2.545 ± 0.004 ms | 8.918 ± 0.032 ms |
| massive/k8s-deployment | 7.119 ± 0.039 ms | 2.245 ± 0.004 ms | 1.444 ± 0.004 ms | **473.625 ± 0.686 µs** | 716.170 ± 1.299 µs | 6.637 ± 0.057 ms |
| medium/package-json | 67.937 ± 0.344 ms | 9.416 ± 0.038 ms | 13.379 ± 0.080 ms | — | **5.559 ± 0.027 ms** | 15.371 ± 0.328 ms |
| medium/user-profile | 63.314 ± 0.122 µs | 33.985 ± 0.067 µs | 36.091 ± 0.286 µs | **5.573 ± 0.015 µs** | 15.557 ± 0.065 µs | 41.604 ± 0.068 µs |
| small/config | 25.609 ± 0.113 µs | 9.979 ± 0.021 µs | 17.385 ± 0.040 µs | **2.226 ± 0.008 µs** | 6.913 ± 0.027 µs | 19.042 ± 0.033 µs |
| small/prettierrc | 196.785 ± 0.406 µs | 149.349 ± 0.311 µs | 103.844 ± 0.185 µs | **23.121 ± 0.031 µs** | 54.774 ± 0.068 µs | 206.943 ± 0.492 µs |

### schema compile — peak heap

| Fixture | boon | corvus-json-schema | jsonschema | jsonschema-valid | jsonschemars | valico |
|---|---|---|---|---|---|---|
| generated/deep-nesting | 636.04 KiB | 392.96 KiB | 534.41 KiB | **109.92 KiB** | 152.84 KiB | 4.78 MiB |
| generated/large-array | 66.67 KiB | 45.44 KiB | 34.10 KiB | **13.70 KiB** | 20.43 KiB | 111.42 KiB |
| generated/many-defs | 8.10 MiB | 5.83 MiB | 3.88 MiB | **1.95 MiB** | 2.64 MiB | 13.73 MiB |
| generated/wide-object | 13.73 MiB | 7.74 MiB | 4.03 MiB | **1.78 MiB** | 2.69 MiB | 8.92 MiB |
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
| generated/deep-nesting | 6.249 ± 0.009 µs | 26.250 ± 0.084 µs | 1.974 ± 0.005 µs | **447.419 ± 1.282 ns** | 17.176 ± 0.087 µs | 2.719 ± 0.009 µs | 545.973 ± 1.615 µs |
| generated/large-array | 17.859 ± 0.094 ms | 65.698 ± 0.544 ms | 6.619 ± 0.046 ms | **2.326 ± 0.028 ms** | 75.582 ± 0.597 ms | 8.395 ± 0.218 ms | 240.982 ± 3.922 ms |
| generated/many-defs | 1.685 ± 0.004 ms | 4.289 ± 0.029 ms | 554.363 ± 4.391 µs | **94.143 ± 0.188 µs** | 326.102 ± 1.370 ms | 714.876 ± 4.126 µs | 37.538 ± 0.107 ms |
| generated/wide-object | 169.285 ± 0.323 µs | 396.906 ± 2.787 µs | 108.140 ± 2.035 µs | 161.651 ± 1.749 µs | 438.649 ± 1.070 µs | **52.752 ± 0.798 µs** | 1.153 ± 0.006 ms |
| large/api-catalog | 563.516 ± 2.544 ns | 2.010 ± 0.012 µs | 206.713 ± 0.996 ns | **89.004 ± 0.809 ns** | 26.124 ± 0.066 µs | 398.354 ± 13.814 ns | 6.052 ± 0.072 µs |
| large/circleci | 2.120 ± 0.045 µs | 11.148 ± 0.053 µs | 421.031 ± 1.281 ns | **185.211 ± 0.966 ns** | 161.341 ± 0.335 µs | 1.011 ± 0.018 µs | 18.437 ± 0.082 µs |
| massive/data-warehouse | 69.169 ± 0.156 µs | 50.250 ± 0.414 ms | 21.388 ± 0.182 µs | **6.622 ± 0.026 µs** | 26.596 ± 0.082 ms | 46.420 ± 0.349 µs | 171.469 ± 0.492 ms |
| massive/k8s-deployment | 514.147 ± 3.457 ns | 2.373 ± 0.022 µs | 182.232 ± 1.213 ns | **52.718 ± 0.445 ns** | 2.891 ± 0.015 µs | 469.821 ± 2.427 ns | 7.376 ± 0.027 µs |
| medium/package-json | 539.849 ± 1.481 ns | 2.589 ± 0.032 µs | **194.525 ± 1.541 ns** | 735.035 ± 5.684 ns | — | 484.594 ± 3.306 ns | 5.212 ± 0.020 µs |
| medium/user-profile | 826.544 ± 5.399 ns | 3.124 ± 0.019 µs | 338.864 ± 1.919 ns | **160.151 ± 0.889 ns** | 12.020 ± 0.031 µs | 456.499 ± 1.254 ns | 7.997 ± 0.045 µs |
| small/config | 222.182 ± 0.959 ns | 817.709 ± 3.418 ns | 100.465 ± 0.268 ns | **57.316 ± 0.253 ns** | 8.689 ± 0.024 µs | 134.035 ± 0.727 ns | 1.667 ± 0.007 µs |
| small/prettierrc | 705.362 ± 4.428 ns | 4.472 ± 0.026 µs | 162.612 ± 0.422 ns | **47.492 ± 0.181 ns** | 8.054 ± 0.027 µs | 281.985 ± 1.617 ns | 6.585 ± 0.023 µs |

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
| medium/user-profile | 0.19 KiB | 1.23 KiB | 0.03 KiB | 0.03 KiB | 8.74 KiB | **0.00 KiB** | 1.87 KiB |
| small/config | 0.19 KiB | 1.18 KiB | 0.03 KiB | 0.03 KiB | 10.18 KiB | **0.00 KiB** | 0.60 KiB |
| small/prettierrc | 0.77 KiB | 1.49 KiB | 0.03 KiB | 0.03 KiB | 2.05 KiB | **0.00 KiB** | 1.71 KiB |

### validate (invalid) — time

| Fixture | boon | corvus-json-schema | jsonschema | jsonschema-macros | jsonschema-valid | jsonschemars | valico |
|---|---|---|---|---|---|---|---|
| generated/deep-nesting | 7.131 ± 0.020 µs | 30.911 ± 0.081 µs | **2.504 ± 0.009 µs** | 13.172 ± 0.292 µs | 21.265 ± 0.042 µs | 4.765 ± 0.029 µs | 550.095 ± 8.270 µs |
| generated/large-array | 17.915 ± 0.087 ms | 65.700 ± 0.223 ms | 6.626 ± 0.087 ms | **4.610 ± 0.021 ms** | 75.588 ± 0.265 ms | 8.348 ± 0.092 ms | 236.021 ± 4.535 ms |
| generated/many-defs | 1.678 ± 0.005 ms | 4.300 ± 0.032 ms | 554.208 ± 1.036 µs | **116.572 ± 0.199 µs** | 327.694 ± 4.023 ms | 719.596 ± 4.011 µs | 37.554 ± 0.127 ms |
| generated/wide-object | 168.564 ± 0.462 µs | 397.218 ± 1.041 µs | 108.939 ± 1.727 µs | 163.590 ± 2.570 µs | 442.318 ± 3.161 µs | **52.962 ± 0.115 µs** | 1.157 ± 0.010 ms |
| large/api-catalog | 635.297 ± 1.457 ns | 2.235 ± 0.013 µs | 352.390 ± 1.067 ns | **279.888 ± 1.568 ns** | 19.339 ± 0.051 µs | 959.057 ± 5.071 ns | 3.964 ± 0.016 µs |
| large/circleci | 124.701 ± 0.395 ns | 509.315 ± 2.244 ns | **116.867 ± 1.853 ns** | 134.521 ± 1.061 ns | 519.402 ± 1.501 ns | 241.198 ± 1.373 ns | 399.215 ± 1.876 ns |
| massive/data-warehouse | 170.365 ± 0.665 µs | 347.719 ± 1.248 µs | 130.714 ± 0.375 µs | **106.805 ± 0.970 µs** | 44.978 ± 0.255 ms | 259.050 ± 1.256 µs | 808.859 ± 1.565 µs |
| massive/k8s-deployment | 177.921 ± 0.416 ns | 636.351 ± 3.373 ns | 114.810 ± 0.656 ns | **106.862 ± 0.803 ns** | 207.687 ± 0.362 µs | 230.609 ± 0.753 ns | 796.045 ± 1.959 ns |
| medium/package-json | 169.933 ± 0.490 ns | 670.772 ± 3.855 ns | **129.923 ± 1.052 ns** | 335.887 ± 1.264 ns | — | 227.152 ± 0.753 ns | 749.277 ± 2.472 ns |
| medium/user-profile | 356.014 ± 1.023 ns | 1.615 ± 0.015 µs | 266.892 ± 1.301 ns | **257.356 ± 5.035 ns** | 7.571 ± 0.052 µs | 485.156 ± 5.567 ns | 2.137 ± 0.009 µs |
| small/config | 278.249 ± 1.082 ns | 1.298 ± 0.007 µs | 214.851 ± 0.792 ns | **195.673 ± 0.470 ns** | 9.185 ± 0.204 µs | 473.769 ± 1.866 ns | 1.488 ± 0.005 µs |
| small/prettierrc | **501.803 ± 1.526 ns** | 2.233 ± 0.017 µs | 811.867 ± 3.247 ns | 705.063 ± 3.707 ns | 4.834 ± 0.049 µs | 1.380 ± 0.003 µs | 1.752 ± 0.007 µs |

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
| *startup* | 208.552 ± 1.567 ms | 153.665 ± 1.307 ms | 2.239 ± 0.366 ms † | 263.048 ± 1.779 ms | 81.179 ± 0.771 ms |
| generated/deep-nesting | 223.834 ± 2.831 ms | 167.377 ± 1.454 ms | **5.626 ± 0.076 ms** | 288.629 ± 5.558 ms | 105.008 ± 1.016 ms |
| generated/large-array | 217.719 ± 1.522 ms | 159.918 ± 0.672 ms | **2.129 ± 0.082 ms** | 270.970 ± 2.562 ms | 84.661 ± 1.045 ms |
| generated/many-defs | 459.190 ± 4.512 ms | 238.186 ± 2.192 ms | **26.660 ± 0.177 ms** | 439.969 ± 4.908 ms | 430.487 ± 1.713 ms |
| generated/wide-object | 600.870 ± 2.371 ms | 284.656 ± 1.608 ms | **23.876 ± 0.162 ms** | 466.186 ± 4.760 ms | 500.224 ± 3.060 ms |
| large/api-catalog | 226.666 ± 1.485 ms | 163.948 ± 1.272 ms | **2.672 ± 0.094 ms** | 285.477 ± 2.141 ms | 93.173 ± 0.843 ms |
| large/circleci | 292.597 ± 1.930 ms | 200.483 ± 1.376 ms | **10.640 ± 0.465 ms** | — | — |
| massive/data-warehouse | 1.029 ± 0.003 s | 321.940 ± 1.758 ms | **41.761 ± 0.364 ms** | 600.780 ± 3.488 ms | 774.895 ± 4.206 ms |
| massive/k8s-deployment | 424.991 ± 2.790 ms | 263.251 ± 2.011 ms | **20.550 ± 0.083 ms** | 411.946 ± 5.712 ms | 85.652 ± 1.431 ms |
| medium/package-json | — | — | 94.097 ± 0.466 ms | — | — |
| medium/user-profile | 220.846 ± 1.683 ms | 160.044 ± 1.555 ms | **2.297 ± 0.037 ms** | 277.099 ± 2.574 ms | 88.290 ± 2.156 ms |
| small/config | 211.939 ± 1.409 ms | 156.605 ± 1.327 ms | **2.029 ± 0.282 ms †** | 270.448 ± 2.475 ms | 85.174 ± 0.808 ms |
| small/prettierrc | 255.790 ± 2.981 ms | 169.781 ± 1.515 ms | **3.272 ± 0.102 ms** | 293.269 ± 1.962 ms | — |

### codegen — peak RSS

| Fixture | datamodel-code-generator | json-schema-to-typescript | jsonschemars | quicktype | typify |
|---|---|---|---|---|---|
| *startup* | 58.36 ± 0.02 MiB | 73.41 ± 0.04 MiB | 3.56 ± 0.01 MiB | 126.88 ± 0.24 MiB | 31.66 ± 0.01 MiB |
| generated/deep-nesting | 60.81 MiB | 78.78 ± 0.33 MiB | **7.48 MiB** | 167.38 ± 0.17 MiB | 37.88 ± 0.02 MiB |
| generated/large-array | 60.12 ± 0.23 MiB | 75.22 ± 0.02 MiB | **4.41 MiB** | 130.47 ± 0.06 MiB | 33.14 ± 0.02 MiB |
| generated/many-defs | 82.75 ± 0.08 MiB | 122.20 ± 0.32 MiB | **22.31 ± 0.03 MiB** | 218.42 MiB | 98.02 MiB |
| generated/wide-object | 97.33 MiB | 166.53 ± 0.50 MiB | **23.20 MiB** | 232.17 ± 0.22 MiB | 115.03 ± 0.47 MiB |
| large/api-catalog | 62.66 ± 0.66 MiB | 77.02 MiB | **5.20 MiB** | 142.16 ± 0.10 MiB | 34.52 ± 0.06 MiB |
| large/circleci | 69.77 MiB | 91.31 MiB | **11.06 ± 0.02 MiB** | — | — |
| massive/data-warehouse | 175.39 ± 0.24 MiB | 177.86 MiB | **29.97 MiB** | 233.67 ± 2.24 MiB | 175.75 ± 0.02 MiB |
| massive/k8s-deployment | 87.39 MiB | 128.97 ± 0.79 MiB | **17.61 ± 0.01 MiB** | 216.00 ± 7.26 MiB | 31.66 ± 0.02 MiB |
| medium/package-json | — | — | 41.50 MiB | — | — |
| medium/user-profile | 60.30 ± 0.16 MiB | 75.06 ± 0.07 MiB | **4.69 MiB** | 133.14 ± 0.02 MiB | 33.41 MiB |
| small/config | 58.64 ± 0.02 MiB | 74.92 ± 0.16 MiB | **4.12 ± 0.01 MiB** | 127.59 MiB | 32.78 MiB |
| small/prettierrc | 66.66 ± 0.13 MiB | 76.91 ± 0.31 MiB | **5.47 ± 0.01 MiB** | 148.45 ± 0.01 MiB | — |

### validate (valid) — time

| Fixture | ajv | blaze | boon | fastjsonschema | jsonschema-cli | jsonschemars | python-jsonschema |
|---|---|---|---|---|---|---|---|
| *startup* | 112.197 ± 1.117 ms | 4.306 ± 0.844 ms † | 3.206 ± 0.495 ms † | 24.455 ± 0.296 ms | 2.833 ± 0.038 ms | 1.572 ± 0.043 ms | 58.927 ± 1.168 ms |
| generated/deep-nesting | 134.863 ± 1.649 ms | 13.882 ± 0.100 ms | 3.933 ± 0.047 ms | 94.561 ± 0.587 ms | 3.390 ± 0.043 ms | **1.803 ± 0.090 ms** | 73.770 ± 0.840 ms |
| generated/large-array | 158.329 ± 1.238 ms | 100.320 ± 0.876 ms | 58.099 ± 0.409 ms | 146.324 ± 0.843 ms | 39.726 ± 0.909 ms | 40.676 ± 0.617 ms | 1.379 ± 0.013 s |
| generated/many-defs | 240.396 ± 1.116 ms | 116.041 ± 0.489 ms | 22.908 ± 0.360 ms | 386.330 ± 1.983 ms | 10.158 ± 0.187 ms | **7.208 ± 0.145 ms** | 415.565 ± 2.407 ms |
| generated/wide-object | — | 58.678 ± 0.317 ms | 27.481 ± 0.301 ms | 305.649 ± 2.280 ms | 7.309 ± 0.148 ms | **3.876 ± 0.039 ms** | 296.022 ± 1.776 ms |
| large/api-catalog | 124.745 ± 1.442 ms | 4.797 ± 0.053 ms | 3.082 ± 0.090 ms | 36.787 ± 0.690 ms | 3.344 ± 0.041 ms | **1.746 ± 0.035 ms** | 65.677 ± 0.820 ms |
| large/circleci | 165.553 ± 1.773 ms | 28.435 ± 0.232 ms | 6.671 ± 0.050 ms | 325.853 ± 2.413 ms | 5.009 ± 0.063 ms | **2.537 ± 0.037 ms** | 74.137 ± 0.722 ms |
| massive/data-warehouse | 556.037 ± 1.610 ms | 240.120 ± 1.050 ms | 49.342 ± 0.337 ms | 1.237 ± 0.005 s | 10.783 ± 0.167 ms | **5.294 ± 0.031 ms** | 457.808 ± 5.488 ms |
| massive/k8s-deployment | — | — | 11.612 ± 0.172 ms | 359.316 ± 0.887 ms | 392.340 ± 57.588 ms † | **2.942 ± 0.053 ms** | 219.186 ± 1.733 ms |
| medium/package-json | 847.625 ± 39.090 ms | 360.393 ± 2.658 ms | 74.258 ± 0.514 ms | 885.358 ± 2.965 ms | — | **7.416 ± 0.091 ms** | 68.787 ± 0.545 ms |
| medium/user-profile | 122.212 ± 2.127 ms | 3.906 ± 0.078 ms | 2.991 ± 0.040 ms | 28.894 ± 0.328 ms | 3.341 ± 0.144 ms | **1.771 ± 0.038 ms** | 62.038 ± 0.572 ms |
| small/config | 121.286 ± 1.414 ms | 3.402 ± 0.231 ms | 2.940 ± 0.105 ms | 25.833 ± 0.138 ms | 3.230 ± 0.124 ms | **1.726 ± 0.042 ms** | 60.215 ± 0.730 ms |
| small/prettierrc | 124.586 ± 1.267 ms | 5.858 ± 0.050 ms | 3.116 ± 0.042 ms | 38.385 ± 0.223 ms | 3.271 ± 0.037 ms | **1.798 ± 0.032 ms** | 60.992 ± 1.018 ms |

### validate (valid) — peak RSS

| Fixture | ajv | blaze | boon | fastjsonschema | jsonschema-cli | jsonschemars | python-jsonschema |
|---|---|---|---|---|---|---|---|
| *startup* | 53.25 ± 0.02 MiB | 8.50 MiB | 5.41 ± 0.01 MiB | 17.19 ± 0.02 MiB | 8.14 ± 0.01 MiB | 3.28 MiB | 24.72 ± 0.02 MiB |
| generated/deep-nesting | 62.48 ± 0.05 MiB | 18.09 MiB | 6.70 ± 0.01 MiB | 87.55 ± 0.09 MiB | 9.83 MiB | **3.91 ± 0.01 MiB** | 25.02 ± 0.05 MiB |
| generated/large-array | 84.19 ± 0.04 MiB | 300.80 MiB | **55.62 ± 0.01 MiB** | 60.20 ± 0.02 MiB | 64.47 MiB | 59.48 ± 0.01 MiB | 65.61 MiB |
| generated/many-defs | 80.72 ± 0.13 MiB | 84.83 MiB | 18.89 ± 0.01 MiB | 327.05 ± 0.27 MiB | 21.12 ± 0.03 MiB | **13.00 ± 0.02 MiB** | 28.22 MiB |
| generated/wide-object | — | 26.86 MiB | 14.94 MiB | 278.08 ± 0.79 MiB | 15.22 MiB | **7.11 ± 0.02 MiB** | 26.20 ± 0.05 MiB |
| large/api-catalog | 55.55 MiB | 9.55 MiB | 5.75 ± 0.01 MiB | 27.44 ± 0.04 MiB | 10.12 ± 0.01 MiB | **3.78 MiB** | 24.66 ± 0.03 MiB |
| large/circleci | 75.38 ± 0.22 MiB | 19.53 MiB | 8.86 ± 0.01 MiB | 270.16 MiB | 12.94 MiB | **5.36 ± 0.02 MiB** | 25.20 ± 0.02 MiB |
| massive/data-warehouse | 149.39 ± 0.14 MiB | 297.67 ± 0.01 MiB | 21.80 ± 0.01 MiB | 971.47 ± 0.03 MiB | 20.70 ± 0.02 MiB | **9.31 ± 0.01 MiB** | 27.41 ± 0.19 MiB |
| massive/k8s-deployment | — | — | 11.59 ± 0.01 MiB | 313.25 MiB | 19.70 MiB | **5.84 MiB** | 26.34 ± 0.02 MiB |
| medium/package-json | 362.38 ± 0.25 MiB | 68.64 MiB | 27.47 ± 0.01 MiB | 658.20 ± 0.12 MiB | — | **11.36 ± 0.02 MiB** | 28.19 MiB |
| medium/user-profile | 54.67 MiB | 9.22 MiB | 5.56 MiB | 20.48 MiB | 9.81 ± 0.01 MiB | **3.62 ± 0.01 MiB** | 24.67 ± 0.02 MiB |
| small/config | 54.30 ± 0.01 MiB | 8.91 MiB | 5.58 ± 0.01 MiB | 17.97 ± 0.04 MiB | 9.66 MiB | **3.53 MiB** | 24.73 MiB |
| small/prettierrc | 58.02 ± 0.13 MiB | 10.09 MiB | 5.66 ± 0.01 MiB | 30.03 ± 0.01 MiB | 9.08 ± 0.01 MiB | **3.70 ± 0.01 MiB** | 24.67 ± 0.01 MiB |

### validate (invalid) — time

| Fixture | ajv | blaze | boon | fastjsonschema | jsonschema-cli | jsonschemars | python-jsonschema |
|---|---|---|---|---|---|---|---|
| *startup* | 112.495 ± 1.387 ms | 3.133 ± 0.048 ms | 2.645 ± 0.032 ms | 24.308 ± 0.185 ms | 2.797 ± 0.034 ms | 1.560 ± 0.036 ms | 58.593 ± 0.663 ms |
| generated/deep-nesting | 134.884 ± 0.954 ms | 35.285 ± 0.712 ms | 3.902 ± 0.044 ms | 94.804 ± 0.650 ms | 3.369 ± 0.042 ms | **1.772 ± 0.023 ms** | 73.761 ± 0.874 ms |
| generated/large-array | 158.254 ± 0.971 ms | 116.614 ± 0.695 ms | 58.044 ± 0.378 ms | 146.464 ± 0.808 ms | 39.775 ± 0.630 ms | 40.739 ± 0.421 ms | 1.396 ± 0.007 s |
| generated/many-defs | 239.646 ± 1.102 ms | 120.516 ± 0.530 ms | 22.971 ± 0.371 ms | 385.911 ± 2.118 ms | 10.170 ± 0.093 ms | **7.079 ± 0.075 ms** | 416.006 ± 2.437 ms |
| generated/wide-object | — | 58.438 ± 0.431 ms | 28.151 ± 0.631 ms | 306.666 ± 1.558 ms | 7.217 ± 0.068 ms | **3.832 ± 0.142 ms** | 296.848 ± 2.021 ms |
| large/api-catalog | 125.466 ± 2.034 ms | 5.165 ± 0.073 ms | 3.081 ± 0.037 ms | 37.076 ± 0.560 ms | 3.434 ± 0.111 ms | **1.761 ± 0.027 ms** | 65.477 ± 0.908 ms |
| large/circleci | 163.309 ± 1.013 ms | 28.722 ± 0.594 ms | 6.642 ± 0.146 ms | 325.470 ± 2.060 ms | 4.969 ± 0.052 ms | **2.534 ± 0.060 ms** | 73.828 ± 0.827 ms |
| massive/data-warehouse | 495.090 ± 2.114 ms | 305.403 ± 1.829 ms | 50.647 ± 1.063 ms | 1.237 ± 0.003 s | 10.947 ± 0.170 ms | **5.445 ± 0.056 ms** | 461.915 ± 2.507 ms |
| massive/k8s-deployment | — | — | 11.629 ± 0.132 ms | 359.315 ± 1.743 ms | 313.279 ± 34.362 ms † | **2.893 ± 0.034 ms** | 220.435 ± 1.683 ms |
| medium/package-json | 845.845 ± 33.668 ms | 359.597 ± 2.327 ms | 74.025 ± 0.442 ms | 884.073 ± 6.112 ms | — | **7.426 ± 0.032 ms** | 68.966 ± 0.511 ms |
| medium/user-profile | 122.084 ± 1.361 ms | 4.025 ± 0.092 ms | 2.992 ± 0.067 ms | 28.668 ± 0.137 ms | 3.340 ± 0.176 ms | **1.748 ± 0.089 ms** | 62.164 ± 0.636 ms |
| small/config | 121.209 ± 2.106 ms | 3.507 ± 0.045 ms | 2.869 ± 0.060 ms | 26.094 ± 0.502 ms | 3.219 ± 0.082 ms | **1.704 ± 0.074 ms** | 60.127 ± 0.665 ms |
| small/prettierrc | 123.928 ± 0.967 ms | 6.410 ± 0.231 ms | 3.106 ± 0.057 ms | 38.643 ± 0.440 ms | 3.287 ± 0.042 ms | **1.805 ± 0.039 ms** | 60.928 ± 0.715 ms |

### validate (invalid) — peak RSS

| Fixture | ajv | blaze | boon | fastjsonschema | jsonschema-cli | jsonschemars | python-jsonschema |
|---|---|---|---|---|---|---|---|
| *startup* | 53.38 ± 0.02 MiB | 8.67 MiB | 5.39 ± 0.01 MiB | 17.22 MiB | 8.28 MiB | 3.30 MiB | 24.64 ± 0.01 MiB |
| generated/deep-nesting | 62.55 MiB | 18.27 MiB | 6.70 ± 0.01 MiB | 87.33 ± 0.02 MiB | 9.92 MiB | **3.92 ± 0.01 MiB** | 25.05 MiB |
| generated/large-array | 84.27 ± 0.06 MiB | 326.53 MiB | **55.59 MiB** | 60.20 ± 0.01 MiB | 64.58 MiB | 59.48 MiB | 65.62 ± 0.02 MiB |
| generated/many-defs | 80.62 MiB | 85.44 MiB | 20.39 MiB | 327.06 MiB | 21.27 MiB | **13.03 ± 0.02 MiB** | 28.25 ± 0.03 MiB |
| generated/wide-object | — | 27.56 MiB | 16.66 ± 0.78 MiB | 278.00 MiB | 15.30 ± 0.00 MiB | **7.11 ± 0.01 MiB** | 26.25 ± 0.05 MiB |
| large/api-catalog | 55.66 ± 0.07 MiB | 9.69 MiB | 5.75 ± 0.01 MiB | 27.39 MiB | 10.20 ± 0.00 MiB | **3.81 MiB** | 24.75 ± 0.06 MiB |
| large/circleci | 75.16 ± 0.03 MiB | 19.48 ± 0.01 MiB | 8.80 ± 0.02 MiB | 271.66 ± 0.68 MiB | 12.80 MiB | **5.36 ± 0.02 MiB** | 25.16 ± 0.04 MiB |
| massive/data-warehouse | 134.09 ± 0.22 MiB | 68.66 MiB | 22.53 ± 0.05 MiB | 971.47 ± 0.07 MiB | 20.77 ± 0.04 MiB | **9.48 ± 0.01 MiB** | 28.12 ± 0.03 MiB |
| massive/k8s-deployment | — | — | 11.59 ± 0.01 MiB | 313.30 ± 0.54 MiB | 19.75 MiB | **5.86 MiB** | 26.66 ± 0.12 MiB |
| medium/package-json | 362.84 MiB | 68.72 MiB | 28.55 MiB | 658.28 ± 2.39 MiB | — | **11.44 ± 0.03 MiB** | 28.16 ± 0.01 MiB |
| medium/user-profile | 54.72 ± 0.01 MiB | 9.31 MiB | 5.56 MiB | 20.47 ± 0.01 MiB | 9.89 ± 0.01 MiB | **3.66 MiB** | 24.67 ± 0.02 MiB |
| small/config | 54.38 ± 0.02 MiB | 9.06 MiB | 5.56 ± 0.02 MiB | 18.03 MiB | 9.77 MiB | **3.59 MiB** | 24.67 ± 0.03 MiB |
| small/prettierrc | 58.06 MiB | 10.20 ± 0.01 MiB | 5.64 ± 0.01 MiB | 30.06 ± 0.04 MiB | 9.20 ± 0.02 MiB | **3.73 MiB** | 24.69 MiB |

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
