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
| generated/deep-nesting | 817.741 ± 18.696 µs | 3.363 ± 0.117 ms | **443.439 ± 2.942 µs** | 10.564 ± 0.206 ms |
| generated/large-array | 71.095 ± 1.876 µs | 145.257 ± 0.818 µs | **7.666 ± 0.038 µs** | 1.119 ± 0.015 ms |
| generated/many-defs | 6.433 ± 0.069 ms | 21.976 ± 0.203 ms | **209.067 ± 0.840 µs** | 132.098 ± 1.346 ms |
| generated/wide-object | 9.348 ± 0.065 ms | 19.420 ± 0.108 ms | **8.978 ± 0.056 ms** | 160.902 ± 4.090 ms |
| large/api-catalog | 225.011 ± 2.902 µs | 495.900 ± 3.620 µs | **26.109 ± 0.084 µs** | 3.381 ± 0.033 ms |
| large/circleci | — | 7.432 ± 0.070 ms | **2.246 ± 0.014 ms** | — |
| massive/data-warehouse | **13.193 ± 0.086 ms** | 40.121 ± 0.551 ms | — | 291.241 ± 3.466 ms |
| massive/k8s-deployment | — | 16.944 ± 0.156 ms | 10.836 ± 0.098 ms | **1.928 ± 0.010 ms** |
| medium/package-json | — | 48.875 ± 0.800 ms | — | — |
| medium/user-profile | 98.358 ± 2.152 µs | 199.342 ± 0.369 µs | **35.274 ± 0.065 µs** | 1.537 ± 0.008 ms |
| small/config | 47.112 ± 0.874 µs | 68.990 ± 0.231 µs | **20.112 ± 0.082 µs** | 546.390 ± 3.772 µs |
| small/prettierrc | **266.310 ± 2.470 µs** | 923.179 ± 2.295 µs | 894.869 ± 2.665 µs | — |

### codegen — peak heap

| Fixture | jsonschema_code_generator | jsonschemars | schemafy | typify |
|---|---|---|---|---|
| generated/deep-nesting | **468.85 KiB** | 1.39 MiB | 680.02 KiB | 12.65 MiB |
| generated/large-array | 38.63 KiB | 81.24 KiB | **19.86 KiB** | 1.23 MiB |
| generated/many-defs | 5.97 MiB | 10.85 MiB | **25.63 KiB** | 133.38 MiB |
| generated/wide-object | **4.60 MiB** | 10.44 MiB | 7.73 MiB | 162.36 MiB |
| large/api-catalog | 157.72 KiB | 285.27 KiB | **31.92 KiB** | 3.64 MiB |
| large/circleci | — | 3.91 MiB | **2.43 MiB** | — |
| massive/data-warehouse | **9.03 MiB** | 16.87 MiB | — | 288.90 MiB |
| massive/k8s-deployment | — | 8.42 MiB | 10.36 MiB | **2.79 MiB** |
| medium/package-json | — | 32.87 MiB | — | — |
| medium/user-profile | 59.96 KiB | 116.96 KiB | **40.33 KiB** | 1.71 MiB |
| small/config | **20.63 KiB** | 40.74 KiB | 29.54 KiB | 639.58 KiB |
| small/prettierrc | **197.95 KiB** | 531.44 KiB | 995.53 KiB | — |

### schema compile — time

| Fixture | boon | corvus-json-schema | jsonschema | jsonschema-valid | jsonschemars | valico |
|---|---|---|---|---|---|---|
| generated/deep-nesting | 1.088 ± 0.007 ms | 263.286 ± 0.535 µs | 221.357 ± 1.169 µs | **68.512 ± 0.226 µs** | 108.022 ± 0.205 µs | 1.624 ± 0.009 ms |
| generated/large-array | 47.776 ± 0.135 µs | 27.109 ± 0.289 µs | 21.652 ± 0.048 µs | **4.419 ± 0.017 µs** | 11.356 ± 0.016 µs | 37.473 ± 0.132 µs |
| generated/many-defs | 14.682 ± 0.143 ms | 3.296 ± 0.003 ms | 3.232 ± 0.022 ms | **675.389 ± 3.373 µs** | 1.738 ± 0.005 ms | 4.564 ± 0.029 ms |
| generated/wide-object | 24.878 ± 0.092 ms | 5.784 ± 0.010 ms | 2.683 ± 0.024 ms | **757.875 ± 3.099 µs** | 1.394 ± 0.003 ms | 3.563 ± 0.025 ms |
| large/api-catalog | 188.341 ± 0.368 µs | 95.676 ± 0.077 µs | 109.759 ± 0.372 µs | **15.692 ± 0.034 µs** | 46.475 ± 0.145 µs | 143.682 ± 0.692 µs |
| large/circleci | 3.054 ± 0.017 ms | 1.023 ± 0.001 ms | 1.092 ± 0.011 ms | **223.105 ± 0.688 µs** | 543.081 ± 1.124 µs | 4.313 ± 0.027 ms |
| massive/data-warehouse | 48.189 ± 0.397 ms | 6.930 ± 0.079 ms | 6.920 ± 0.089 ms | **1.249 ± 0.006 ms** | 2.823 ± 0.012 ms | 10.583 ± 0.132 ms |
| massive/k8s-deployment | 7.832 ± 0.020 ms | 2.357 ± 0.190 ms | 1.723 ± 0.011 ms | **537.784 ± 2.181 µs** | 806.252 ± 2.899 µs | 7.910 ± 0.045 ms |
| medium/package-json | 76.773 ± 0.597 ms | 10.898 ± 0.451 ms | 17.060 ± 0.289 ms | — | **6.472 ± 0.062 ms** | 19.729 ± 0.205 ms |
| medium/user-profile | 71.678 ± 0.152 µs | 37.516 ± 0.427 µs | 42.097 ± 0.051 µs | **6.334 ± 0.044 µs** | 17.413 ± 0.059 µs | 48.641 ± 0.198 µs |
| small/config | 28.611 ± 0.203 µs | 11.201 ± 0.140 µs | 20.144 ± 0.069 µs | **2.567 ± 0.016 µs** | 7.993 ± 0.067 µs | 21.583 ± 0.056 µs |
| small/prettierrc | 215.374 ± 0.450 µs | 159.497 ± 0.309 µs | 116.782 ± 0.280 µs | **26.063 ± 0.087 µs** | 60.172 ± 0.425 µs | 243.063 ± 0.860 µs |

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
| generated/deep-nesting | 7.437 ± 0.061 µs | 27.888 ± 0.376 µs | 2.476 ± 0.032 µs | **587.526 ± 17.076 ns** | 19.789 ± 0.078 µs | 3.185 ± 0.031 µs | 588.424 ± 1.428 µs |
| generated/large-array | 19.872 ± 0.131 ms | 71.214 ± 0.697 ms | 8.012 ± 0.119 ms | **2.508 ± 0.006 ms** | 89.528 ± 0.643 ms | 9.501 ± 0.133 ms | 301.816 ± 5.889 ms |
| generated/many-defs | 1.914 ± 0.014 ms | 4.593 ± 0.014 ms | 674.423 ± 6.619 µs | **108.174 ± 1.951 µs** | 334.273 ± 2.124 ms | 808.395 ± 6.835 µs | 42.929 ± 0.385 ms |
| generated/wide-object | 182.092 ± 1.475 µs | 424.251 ± 0.953 µs | 120.998 ± 2.305 µs | 159.960 ± 1.827 µs | 502.814 ± 3.071 µs | **57.208 ± 0.372 µs** | 1.336 ± 0.011 ms |
| large/api-catalog | 586.936 ± 5.273 ns | 2.167 ± 0.057 µs | 249.453 ± 4.171 ns | **94.961 ± 1.086 ns** | 29.229 ± 0.356 µs | 419.869 ± 5.100 ns | 6.854 ± 0.184 µs |
| large/circleci | 2.324 ± 0.013 µs | 12.111 ± 0.131 µs | 496.346 ± 2.899 ns | **206.312 ± 4.094 ns** | 176.938 ± 2.660 µs | 1.070 ± 0.009 µs | 21.190 ± 0.126 µs |
| massive/data-warehouse | 75.264 ± 0.846 µs | 54.982 ± 0.701 ms | 26.061 ± 0.411 µs | **7.257 ± 0.168 µs** | 28.218 ± 0.418 ms | 50.540 ± 1.131 µs | 202.977 ± 4.460 ms |
| massive/k8s-deployment | 556.303 ± 15.896 ns | 2.557 ± 0.024 µs | 224.298 ± 3.606 ns | **54.707 ± 0.455 ns** | 3.292 ± 0.086 µs | 528.685 ± 12.138 ns | 8.395 ± 0.216 µs |
| medium/package-json | 541.068 ± 7.186 ns | 2.784 ± 0.021 µs | **239.756 ± 8.285 ns** | 746.809 ± 9.928 ns | — | 514.785 ± 8.687 ns | 5.649 ± 0.079 µs |
| medium/user-profile | 857.632 ± 11.035 ns | 3.363 ± 0.044 µs | 394.265 ± 8.809 ns | **169.846 ± 1.717 ns** | 12.970 ± 0.156 µs | 529.069 ± 14.993 ns | 8.704 ± 0.127 µs |
| small/config | 234.416 ± 4.168 ns | 898.327 ± 13.425 ns | 119.170 ± 2.593 ns | **62.318 ± 0.320 ns** | 9.239 ± 0.151 µs | 141.337 ± 1.418 ns | 1.896 ± 0.033 µs |
| small/prettierrc | 735.459 ± 10.386 ns | 4.861 ± 0.050 µs | 185.584 ± 4.658 ns | **52.052 ± 0.381 ns** | 9.169 ± 0.170 µs | 283.316 ± 7.270 ns | 7.492 ± 0.074 µs |

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
| massive/k8s-deployment | 0.19 KiB | 1.59 KiB | 0.03 KiB | 0.03 KiB | 2.07 KiB | **0.00 KiB** | 1.99 KiB |
| medium/package-json | 0.19 KiB | 1.23 KiB | 0.03 KiB | 0.03 KiB | — | **0.00 KiB** | 1.20 KiB |
| medium/user-profile | 0.19 KiB | 1.23 KiB | 0.03 KiB | 0.03 KiB | 8.74 KiB | **0.00 KiB** | 1.87 KiB |
| small/config | 0.19 KiB | 1.18 KiB | 0.03 KiB | 0.03 KiB | 10.18 KiB | **0.00 KiB** | 0.60 KiB |
| small/prettierrc | 0.77 KiB | 1.49 KiB | 0.03 KiB | 0.03 KiB | 2.05 KiB | **0.00 KiB** | 1.71 KiB |

### validate (invalid) — time

| Fixture | boon | corvus-json-schema | jsonschema | jsonschema-macros | jsonschema-valid | jsonschemars | valico |
|---|---|---|---|---|---|---|---|
| generated/deep-nesting | 7.890 ± 0.033 µs | 33.278 ± 0.397 µs | **2.815 ± 0.016 µs** | 13.841 ± 0.049 µs | 23.267 ± 0.289 µs | 5.143 ± 0.051 µs | 559.762 ± 4.801 µs |
| generated/large-array | 18.142 ± 0.107 ms | 74.233 ± 2.493 ms | 7.262 ± 0.037 ms | **5.083 ± 0.067 ms** | 83.159 ± 0.639 ms | 8.663 ± 0.111 ms | 249.035 ± 1.150 ms |
| generated/many-defs | 1.742 ± 0.012 ms | 4.591 ± 0.040 ms | 620.118 ± 0.844 µs | **136.336 ± 1.778 µs** | 323.755 ± 7.146 ms | 762.782 ± 2.921 µs | 38.240 ± 0.060 ms |
| generated/wide-object | 171.521 ± 0.404 µs | 427.934 ± 3.749 µs | 111.986 ± 2.425 µs | 151.745 ± 0.241 µs | 470.058 ± 0.626 µs | **54.452 ± 0.096 µs** | 1.235 ± 0.014 ms |
| large/api-catalog | 672.626 ± 6.598 ns | 2.391 ± 0.022 µs | 397.602 ± 5.964 ns | **307.975 ± 0.826 ns** | 20.208 ± 0.133 µs | 1.057 ± 0.014 µs | 4.329 ± 0.025 µs |
| large/circleci | **128.551 ± 0.818 ns** | 577.683 ± 4.888 ns | 134.638 ± 1.055 ns | 147.732 ± 0.304 ns | 553.991 ± 7.293 ns | 269.437 ± 5.343 ns | 467.084 ± 5.380 ns |
| massive/data-warehouse | 181.648 ± 1.276 µs | 368.415 ± 0.437 µs | 139.012 ± 0.341 µs | **105.508 ± 0.267 µs** | 43.863 ± 0.381 ms | 271.723 ± 1.727 µs | 872.712 ± 1.658 µs |
| massive/k8s-deployment | 182.861 ± 1.071 ns | 721.737 ± 8.160 ns | 137.069 ± 2.333 ns | **121.496 ± 0.353 ns** | 234.087 ± 4.024 µs | 243.885 ± 3.363 ns | 903.843 ± 7.565 ns |
| medium/package-json | 174.167 ± 0.998 ns | 774.116 ± 6.668 ns | **154.067 ± 2.009 ns** | 379.374 ± 1.606 ns | — | 245.840 ± 1.465 ns | 850.734 ± 5.163 ns |
| medium/user-profile | 377.421 ± 2.427 ns | 1.727 ± 0.007 µs | 312.268 ± 3.739 ns | **278.445 ± 0.844 ns** | 7.944 ± 0.029 µs | 546.579 ± 8.492 ns | 2.404 ± 0.014 µs |
| small/config | 291.375 ± 1.093 ns | 1.431 ± 0.005 µs | 250.008 ± 3.800 ns | **227.298 ± 3.242 ns** | 9.355 ± 0.022 µs | 533.615 ± 8.105 ns | 1.674 ± 0.007 µs |
| small/prettierrc | **536.020 ± 2.931 ns** | 2.413 ± 0.016 µs | 917.677 ± 7.026 ns | 784.935 ± 3.713 ns | 5.151 ± 0.026 µs | 1.519 ± 0.014 µs | 2.009 ± 0.010 µs |

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
| *startup* | 212.139 ± 2.715 ms | 155.517 ± 1.578 ms | 2.435 ± 0.276 ms † | 265.997 ± 1.846 ms | 84.769 ± 1.406 ms |
| generated/deep-nesting | 229.378 ± 3.416 ms | 170.795 ± 1.150 ms | **6.028 ± 0.122 ms** | 293.669 ± 8.738 ms | 109.178 ± 0.742 ms |
| generated/large-array | 221.885 ± 0.941 ms | 161.409 ± 0.806 ms | **2.148 ± 0.048 ms** | 273.224 ± 2.236 ms | 88.362 ± 1.472 ms |
| generated/many-defs | 458.370 ± 1.963 ms | 237.390 ± 0.606 ms | **28.478 ± 0.561 ms** | 445.179 ± 5.830 ms | 441.608 ± 3.242 ms |
| generated/wide-object | 598.723 ± 1.775 ms | 283.263 ± 1.598 ms | **24.465 ± 0.212 ms** | 465.262 ± 4.563 ms | 502.218 ± 1.295 ms |
| large/api-catalog | 225.396 ± 1.096 ms | 162.112 ± 0.870 ms | **2.595 ± 0.041 ms** | 283.960 ± 2.123 ms | 92.477 ± 0.612 ms |
| large/circleci | 292.005 ± 1.342 ms | 199.172 ± 1.817 ms | **10.871 ± 0.085 ms** | — | — |
| massive/data-warehouse | 1.025 ± 0.004 s | 319.376 ± 2.521 ms | **43.346 ± 0.947 ms** | 598.430 ± 3.014 ms | 779.359 ± 2.218 ms |
| massive/k8s-deployment | 422.460 ± 2.469 ms | 261.274 ± 1.971 ms | **21.337 ± 0.443 ms** | 420.174 ± 10.517 ms | 84.716 ± 0.940 ms |
| medium/package-json | — | — | 97.416 ± 0.372 ms | — | — |
| medium/user-profile | 218.995 ± 1.299 ms | 159.118 ± 1.414 ms | **2.203 ± 0.035 ms** | 274.536 ± 1.546 ms | 87.611 ± 1.238 ms |
| small/config | 211.053 ± 13.399 ms | 155.850 ± 1.801 ms | **1.934 ± 0.024 ms** | 267.884 ± 13.430 ms | 83.815 ± 1.002 ms |
| small/prettierrc | 258.182 ± 2.650 ms | 170.826 ± 1.358 ms | **3.209 ± 0.090 ms** | 294.802 ± 2.244 ms | — |

### codegen — peak RSS

| Fixture | datamodel-code-generator | json-schema-to-typescript | jsonschemars | quicktype | typify |
|---|---|---|---|---|---|
| *startup* | 58.69 ± 0.10 MiB | 73.41 ± 0.73 MiB | 3.70 MiB | 126.52 ± 0.15 MiB | 31.69 MiB |
| generated/deep-nesting | 60.80 ± 0.26 MiB | 78.11 ± 0.03 MiB | **7.64 MiB** | 152.52 ± 0.57 MiB | 37.89 ± 0.02 MiB |
| generated/large-array | 60.03 ± 0.02 MiB | 75.72 MiB | **4.64 ± 0.02 MiB** | 129.94 ± 0.45 MiB | 33.16 ± 0.01 MiB |
| generated/many-defs | 82.59 ± 0.23 MiB | 121.50 ± 0.46 MiB | **22.36 ± 0.01 MiB** | 218.55 ± 0.29 MiB | 97.98 ± 0.03 MiB |
| generated/wide-object | 96.97 ± 0.38 MiB | 166.86 ± 0.24 MiB | **22.52 ± 0.04 MiB** | 232.00 ± 0.16 MiB | 114.55 ± 0.23 MiB |
| large/api-catalog | 62.70 ± 0.42 MiB | 75.70 ± 0.02 MiB | **5.28 MiB** | 142.19 ± 0.14 MiB | 34.56 ± 0.02 MiB |
| large/circleci | 70.17 MiB | 91.23 ± 0.23 MiB | **11.02 MiB** | — | — |
| massive/data-warehouse | 175.72 ± 0.60 MiB | 177.89 ± 0.26 MiB | **31.53 ± 0.12 MiB** | 233.23 ± 0.04 MiB | 175.73 MiB |
| massive/k8s-deployment | 88.19 ± 0.87 MiB | 129.52 ± 0.01 MiB | **17.56 ± 0.29 MiB** | 213.33 ± 0.49 MiB | 31.66 MiB |
| medium/package-json | — | — | 39.28 ± 0.01 MiB | — | — |
| medium/user-profile | 60.20 ± 0.03 MiB | 75.02 ± 0.04 MiB | **4.91 MiB** | 132.86 ± 0.33 MiB | 33.38 MiB |
| small/config | 58.72 ± 0.02 MiB | 74.48 ± 0.01 MiB | **4.36 MiB** | 127.50 ± 0.55 MiB | 32.77 MiB |
| small/prettierrc | 66.50 MiB | 76.81 ± 0.11 MiB | **5.64 ± 0.03 MiB** | 148.28 MiB | — |

### validate (valid) — time

| Fixture | ajv | blaze | boon | fastjsonschema | jsonschema-cli | jsonschemars | python-jsonschema |
|---|---|---|---|---|---|---|---|
| *startup* | 114.404 ± 1.083 ms | 4.011 ± 0.388 ms | 3.248 ± 0.297 ms | 25.250 ± 0.508 ms | 3.773 ± 0.169 ms | 1.639 ± 0.034 ms | 60.114 ± 0.727 ms |
| generated/deep-nesting | 137.933 ± 2.429 ms | 14.298 ± 0.281 ms | 4.162 ± 0.057 ms | 95.288 ± 0.281 ms | 4.218 ± 0.170 ms | **1.818 ± 0.042 ms** | 75.260 ± 0.965 ms |
| generated/large-array | 159.146 ± 0.921 ms | 98.624 ± 0.493 ms | 58.750 ± 0.610 ms | 150.460 ± 1.242 ms | 44.996 ± 0.263 ms | 44.891 ± 0.283 ms | 1.379 ± 0.021 s |
| generated/many-defs | 238.938 ± 0.986 ms | 115.482 ± 0.381 ms | 23.241 ± 0.337 ms | 382.390 ± 0.709 ms | 11.481 ± 0.119 ms | **7.444 ± 0.059 ms** | 414.406 ± 2.745 ms |
| generated/wide-object | — | 58.407 ± 0.453 ms | 27.743 ± 0.068 ms | 303.006 ± 1.310 ms | 8.069 ± 0.073 ms | **3.864 ± 0.037 ms** | 293.778 ± 2.714 ms |
| large/api-catalog | 124.597 ± 0.709 ms | 4.777 ± 0.084 ms | 3.093 ± 0.032 ms | 36.320 ± 0.247 ms | 3.926 ± 0.052 ms | **1.725 ± 0.035 ms** | 65.332 ± 0.404 ms |
| large/circleci | 164.342 ± 2.277 ms | 28.134 ± 0.188 ms | 6.723 ± 0.076 ms | 323.066 ± 3.907 ms | 5.790 ± 0.213 ms | **2.492 ± 0.033 ms** | 73.964 ± 1.137 ms |
| massive/data-warehouse | 552.007 ± 1.791 ms | 238.867 ± 1.328 ms | 49.548 ± 0.299 ms | 1.212 ± 0.002 s | 11.804 ± 0.303 ms | **5.324 ± 0.045 ms** | 457.275 ± 2.019 ms |
| massive/k8s-deployment | — | — | 11.744 ± 0.132 ms | 355.898 ± 0.948 ms | 257.777 ± 66.363 ms † | **2.853 ± 0.033 ms** | 219.149 ± 1.973 ms |
| medium/package-json | 840.876 ± 4.435 ms | 360.538 ± 1.872 ms | 74.185 ± 0.308 ms | 873.113 ± 7.761 ms | — | **7.534 ± 0.106 ms** | 69.783 ± 0.845 ms |
| medium/user-profile | 121.361 ± 1.087 ms | 3.674 ± 0.055 ms | 2.885 ± 0.033 ms | 28.183 ± 0.233 ms | 3.747 ± 0.050 ms | **1.649 ± 0.033 ms** | 61.540 ± 0.397 ms |
| small/config | 120.042 ± 1.117 ms | 3.244 ± 0.101 ms | 2.827 ± 0.037 ms | 25.667 ± 0.383 ms | 3.694 ± 0.068 ms | **1.634 ± 0.020 ms** | 64.131 ± 3.351 ms |
| small/prettierrc | 124.704 ± 1.559 ms | 5.921 ± 0.174 ms | 3.082 ± 0.084 ms | 38.819 ± 0.574 ms | 3.847 ± 0.207 ms | **1.738 ± 0.043 ms** | 61.622 ± 1.785 ms |

### validate (valid) — peak RSS

| Fixture | ajv | blaze | boon | fastjsonschema | jsonschema-cli | jsonschemars | python-jsonschema |
|---|---|---|---|---|---|---|---|
| *startup* | 53.23 ± 0.02 MiB | 8.50 MiB | 5.67 ± 0.01 MiB | 17.19 MiB | 8.45 MiB | 3.48 MiB | 24.67 ± 0.02 MiB |
| generated/deep-nesting | 62.47 ± 0.02 MiB | 18.08 MiB | 6.98 ± 0.00 MiB | 87.36 ± 0.03 MiB | 10.02 ± 0.01 MiB | **4.12 MiB** | 24.98 ± 0.01 MiB |
| generated/large-array | 84.19 ± 1.07 MiB | 300.80 MiB | **55.94 ± 0.02 MiB** | 60.25 ± 0.04 MiB | 64.88 ± 0.01 MiB | 59.67 ± 0.01 MiB | 65.59 MiB |
| generated/many-defs | 80.20 ± 0.09 MiB | 84.83 MiB | 19.20 ± 0.02 MiB | 327.08 ± 0.02 MiB | 21.67 ± 0.02 MiB | **13.14 ± 0.01 MiB** | 28.22 ± 0.02 MiB |
| generated/wide-object | — | 26.86 MiB | 15.20 ± 0.01 MiB | 278.08 MiB | 15.50 MiB | **7.39 MiB** | 26.30 ± 0.07 MiB |
| large/api-catalog | 55.53 ± 0.03 MiB | 9.55 MiB | 6.02 MiB | 27.39 MiB | 10.56 MiB | **3.94 ± 0.01 MiB** | 24.72 ± 0.03 MiB |
| large/circleci | 75.80 ± 0.05 MiB | 19.52 ± 0.01 MiB | 9.20 ± 0.02 MiB | 271.22 ± 0.26 MiB | 13.33 ± 0.04 MiB | **5.55 ± 0.03 MiB** | 25.30 ± 0.06 MiB |
| massive/data-warehouse | 153.97 MiB | 297.67 ± 0.01 MiB | 22.09 ± 0.01 MiB | 971.47 ± 0.03 MiB | 21.27 MiB | **9.50 MiB** | 27.66 ± 0.07 MiB |
| massive/k8s-deployment | — | — | 11.86 MiB | 316.84 ± 1.80 MiB | 20.30 MiB | **6.03 ± 0.01 MiB** | 26.31 ± 0.01 MiB |
| medium/package-json | 361.91 MiB | 68.64 MiB | 27.77 ± 0.01 MiB | 660.44 ± 1.16 MiB | — | **11.61 MiB** | 28.09 ± 0.01 MiB |
| medium/user-profile | 54.61 ± 0.02 MiB | 9.22 MiB | 5.88 ± 0.01 MiB | 20.45 MiB | 10.31 ± 0.01 MiB | **3.80 MiB** | 24.69 MiB |
| small/config | 54.33 ± 0.02 MiB | 8.91 MiB | 5.83 ± 0.01 MiB | 18.00 ± 0.02 MiB | 10.14 ± 0.01 MiB | **3.70 ± 0.01 MiB** | 24.72 ± 0.01 MiB |
| small/prettierrc | 58.08 MiB | 10.09 ± 0.01 MiB | 5.91 MiB | 30.05 ± 0.01 MiB | 9.20 ± 0.01 MiB | **3.92 MiB** | 24.67 ± 0.05 MiB |

### validate (invalid) — time

| Fixture | ajv | blaze | boon | fastjsonschema | jsonschema-cli | jsonschemars | python-jsonschema |
|---|---|---|---|---|---|---|---|
| *startup* | 114.707 ± 2.074 ms | 3.194 ± 0.176 ms | 2.846 ± 0.102 ms | 25.150 ± 0.586 ms | 3.573 ± 0.047 ms | 1.600 ± 0.058 ms | 60.426 ± 2.050 ms |
| generated/deep-nesting | 137.787 ± 1.119 ms | 35.933 ± 0.401 ms | 4.153 ± 0.053 ms | 95.523 ± 1.115 ms | 4.147 ± 0.047 ms | **1.927 ± 0.070 ms** | 75.194 ± 1.452 ms |
| generated/large-array | 159.204 ± 0.753 ms | 114.545 ± 0.937 ms | 58.793 ± 1.120 ms | 150.020 ± 1.788 ms | 44.807 ± 0.824 ms | 44.622 ± 0.876 ms | 1.412 ± 0.014 s |
| generated/many-defs | 239.987 ± 1.455 ms | 119.932 ± 0.458 ms | 23.015 ± 0.331 ms | 383.091 ± 2.203 ms | 11.504 ± 0.167 ms | **7.394 ± 0.109 ms** | 417.263 ± 1.689 ms |
| generated/wide-object | — | 58.394 ± 0.427 ms | 27.841 ± 0.332 ms | 303.881 ± 1.089 ms | 8.115 ± 0.099 ms | **3.808 ± 0.033 ms** | 294.467 ± 1.839 ms |
| large/api-catalog | 124.982 ± 1.891 ms | 5.111 ± 0.059 ms | 3.130 ± 0.068 ms | 36.382 ± 0.224 ms | 3.987 ± 0.072 ms | **1.745 ± 0.033 ms** | 65.246 ± 0.371 ms |
| large/circleci | 162.038 ± 0.704 ms | 28.299 ± 0.192 ms | 6.690 ± 0.095 ms | 322.131 ± 4.639 ms | 5.687 ± 0.089 ms | **2.542 ± 0.046 ms** | 73.839 ± 2.818 ms |
| massive/data-warehouse | 491.587 ± 2.352 ms | 304.810 ± 1.116 ms | 50.672 ± 0.366 ms | 1.217 ± 0.005 s | 11.849 ± 0.116 ms | **5.526 ± 0.076 ms** | 459.567 ± 2.715 ms |
| massive/k8s-deployment | — | — | 11.758 ± 0.120 ms | 359.677 ± 15.407 ms | 273.160 ± 55.212 ms † | **2.855 ± 0.059 ms** | 220.258 ± 1.909 ms |
| medium/package-json | 851.447 ± 31.652 ms | 360.357 ± 1.617 ms | 74.309 ± 0.536 ms | 871.881 ± 3.181 ms | — | **7.442 ± 0.089 ms** | 69.987 ± 2.350 ms |
| medium/user-profile | 121.127 ± 1.022 ms | 3.827 ± 0.080 ms | 2.893 ± 0.046 ms | 28.075 ± 0.208 ms | 3.749 ± 0.039 ms | **1.671 ± 0.029 ms** | 61.559 ± 0.322 ms |
| small/config | 121.002 ± 1.008 ms | 3.411 ± 0.328 ms | 3.076 ± 0.132 ms | 25.535 ± 0.376 ms | 3.752 ± 0.053 ms | **1.641 ± 0.041 ms** | 60.174 ± 1.340 ms |
| small/prettierrc | 125.100 ± 1.374 ms | 6.240 ± 0.176 ms | 3.114 ± 0.051 ms | 38.633 ± 0.389 ms | 3.830 ± 0.084 ms | **1.765 ± 0.034 ms** | 61.575 ± 0.544 ms |

### validate (invalid) — peak RSS

| Fixture | ajv | blaze | boon | fastjsonschema | jsonschema-cli | jsonschemars | python-jsonschema |
|---|---|---|---|---|---|---|---|
| *startup* | 53.31 ± 0.03 MiB | 8.67 MiB | 5.66 ± 0.01 MiB | 17.22 MiB | 8.56 ± 0.01 MiB | 3.53 MiB | 24.84 ± 0.07 MiB |
| generated/deep-nesting | 62.56 ± 0.04 MiB | 18.27 ± 0.00 MiB | 7.03 ± 0.03 MiB | 87.36 ± 0.03 MiB | 10.16 ± 0.01 MiB | **4.19 ± 0.01 MiB** | 25.05 ± 0.01 MiB |
| generated/large-array | 84.02 ± 0.95 MiB | 326.53 MiB | **55.92 ± 0.01 MiB** | 60.27 ± 0.01 MiB | 65.02 MiB | 59.72 MiB | 66.02 MiB |
| generated/many-defs | 81.17 ± 0.40 MiB | 85.44 MiB | 19.19 ± 0.01 MiB | 327.06 ± 0.00 MiB | 21.70 MiB | **13.20 MiB** | 28.22 ± 0.03 MiB |
| generated/wide-object | — | 27.56 ± 0.00 MiB | 15.22 ± 0.45 MiB | 279.95 MiB | 15.64 ± 0.01 MiB | **7.42 ± 0.01 MiB** | 26.25 ± 0.02 MiB |
| large/api-catalog | 55.61 ± 0.01 MiB | 9.69 MiB | 6.03 ± 0.02 MiB | 27.42 ± 0.01 MiB | 10.73 ± 0.02 MiB | **3.97 ± 0.01 MiB** | 24.73 ± 0.05 MiB |
| large/circleci | 75.14 ± 0.04 MiB | 19.48 ± 0.01 MiB | 9.17 ± 0.01 MiB | 270.19 ± 0.70 MiB | 13.27 ± 0.01 MiB | **5.61 ± 0.01 MiB** | 25.47 ± 0.15 MiB |
| massive/data-warehouse | 149.09 ± 5.28 MiB | 68.66 MiB | 22.89 ± 0.03 MiB | 971.70 MiB | 21.28 ± 0.01 MiB | **9.66 ± 0.01 MiB** | 28.08 ± 0.01 MiB |
| massive/k8s-deployment | — | — | 11.84 ± 0.01 MiB | 315.36 ± 0.95 MiB | 20.25 ± 0.06 MiB | **6.06 MiB** | 26.62 ± 0.14 MiB |
| medium/package-json | 361.91 MiB | 68.72 MiB | 27.77 ± 0.00 MiB | 662.38 ± 1.15 MiB | — | **11.62 ± 0.05 MiB** | 28.11 ± 0.00 MiB |
| medium/user-profile | 54.69 ± 0.02 MiB | 9.31 MiB | 5.91 ± 0.01 MiB | 20.48 ± 0.01 MiB | 10.50 ± 0.01 MiB | **3.83 MiB** | 24.67 ± 0.02 MiB |
| small/config | 54.44 ± 0.08 MiB | 9.06 MiB | 5.84 ± 0.01 MiB | 18.03 MiB | 10.33 ± 0.02 MiB | **3.78 MiB** | 24.70 ± 0.04 MiB |
| small/prettierrc | 58.02 ± 0.17 MiB | 10.19 MiB | 5.94 ± 0.01 MiB | 30.09 ± 0.01 MiB | 9.38 ± 0.01 MiB | **3.98 ± 0.02 MiB** | 24.72 ± 0.02 MiB |

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
