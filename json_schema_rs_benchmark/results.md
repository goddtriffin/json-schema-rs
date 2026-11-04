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
| in-process | validate (invalid) | time | jsonschema-macros | 6 / 12 |
| in-process | validate (invalid) | peak heap | jsonschema | 6 / 12 |
| CLI | codegen | time | jsonschemars | 11 / 11 |
| CLI | codegen | peak RSS | jsonschemars | 11 / 11 |
| CLI | validate (valid) | time | jsonschemars | 10 / 12 |
| CLI | validate (valid) | peak RSS | jsonschemars | 11 / 12 |
| CLI | validate (invalid) | time | jsonschemars | 11 / 12 |
| CLI | validate (invalid) | peak RSS | jsonschemars | 11 / 12 |

## In-process (library calls)

### codegen — time

| Fixture | jsonschema_code_generator | jsonschemars | schemafy | typify |
|---|---|---|---|---|
| generated/deep-nesting | 818.090 ± 3.885 µs | 3.193 ± 0.015 ms | **427.028 ± 1.173 µs** | 10.205 ± 0.079 ms |
| generated/large-array | 68.038 ± 0.227 µs | 140.618 ± 0.400 µs | **7.525 ± 0.045 µs** | 1.091 ± 0.006 ms |
| generated/many-defs | 6.283 ± 0.037 ms | 21.594 ± 0.192 ms | **205.769 ± 0.507 µs** | 127.418 ± 1.436 ms |
| generated/wide-object | 9.083 ± 0.059 ms | 19.055 ± 0.073 ms | **8.781 ± 0.080 ms** | 153.935 ± 1.724 ms |
| large/api-catalog | 219.759 ± 1.466 µs | 484.454 ± 6.743 µs | **25.693 ± 0.138 µs** | 3.280 ± 0.034 ms |
| large/circleci | — | 7.275 ± 0.026 ms | **2.192 ± 0.014 ms** | — |
| massive/data-warehouse | **12.886 ± 0.175 ms** | 38.900 ± 0.576 ms | — | 280.516 ± 2.485 ms |
| massive/k8s-deployment | — | 16.544 ± 0.224 ms | 10.695 ± 0.098 ms | **1.906 ± 0.032 ms** |
| medium/package-json | — | 48.394 ± 1.027 ms | — | — |
| medium/user-profile | 92.930 ± 1.013 µs | 194.443 ± 0.926 µs | **34.258 ± 0.060 µs** | 1.504 ± 0.010 ms |
| small/config | 40.491 ± 0.260 µs | 67.484 ± 0.379 µs | **19.693 ± 0.108 µs** | 537.329 ± 2.267 µs |
| small/prettierrc | **251.487 ± 0.792 µs** | 913.938 ± 5.167 µs | 876.068 ± 2.504 µs | — |

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
| generated/deep-nesting | 1.078 ± 0.005 ms | 276.679 ± 1.670 µs | 214.399 ± 0.937 µs | **66.965 ± 0.115 µs** | 105.364 ± 0.299 µs | 1.621 ± 0.010 ms |
| generated/large-array | 47.171 ± 0.199 µs | 28.269 ± 0.059 µs | 21.465 ± 0.070 µs | **4.409 ± 0.030 µs** | 11.167 ± 0.024 µs | 36.930 ± 0.213 µs |
| generated/many-defs | 14.617 ± 0.178 ms | 3.502 ± 0.031 ms | 3.174 ± 0.017 ms | **660.344 ± 3.557 µs** | 1.724 ± 0.006 ms | 4.574 ± 0.039 ms |
| generated/wide-object | 24.336 ± 0.111 ms | 6.091 ± 0.025 ms | 2.654 ± 0.012 ms | **745.708 ± 4.993 µs** | 1.369 ± 0.006 ms | 3.531 ± 0.018 ms |
| large/api-catalog | 187.027 ± 0.470 µs | 100.032 ± 0.506 µs | 107.494 ± 0.409 µs | **15.650 ± 0.127 µs** | 45.509 ± 0.132 µs | 141.919 ± 0.606 µs |
| large/circleci | 3.011 ± 0.012 ms | 1.073 ± 0.011 ms | 1.082 ± 0.009 ms | **224.003 ± 0.884 µs** | 535.062 ± 2.056 µs | 4.274 ± 0.048 ms |
| massive/data-warehouse | 46.910 ± 0.496 ms | 7.402 ± 0.082 ms | 6.865 ± 0.077 ms | **1.237 ± 0.006 ms** | 2.790 ± 0.013 ms | 10.333 ± 0.116 ms |
| massive/k8s-deployment | 7.727 ± 0.049 ms | 2.492 ± 0.028 ms | 1.681 ± 0.010 ms | **524.340 ± 2.488 µs** | 777.426 ± 3.430 µs | 7.850 ± 0.060 ms |
| medium/package-json | 72.364 ± 0.425 ms | 10.450 ± 0.124 ms | 15.375 ± 0.210 ms | — | **6.396 ± 0.074 ms** | 18.069 ± 0.357 ms |
| medium/user-profile | 70.471 ± 0.337 µs | 38.161 ± 0.256 µs | 41.428 ± 0.144 µs | **6.287 ± 0.042 µs** | 17.310 ± 0.039 µs | 47.711 ± 0.187 µs |
| small/config | 28.309 ± 0.094 µs | 11.557 ± 0.047 µs | 19.792 ± 0.092 µs | **2.541 ± 0.018 µs** | 7.926 ± 0.061 µs | 21.363 ± 0.099 µs |
| small/prettierrc | 213.876 ± 0.568 µs | 166.665 ± 0.857 µs | 115.258 ± 0.478 µs | **25.813 ± 0.095 µs** | 59.396 ± 0.678 µs | 239.953 ± 1.029 µs |

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
| generated/deep-nesting | 7.066 ± 0.027 µs | 29.063 ± 0.160 µs | 2.538 ± 0.015 µs | **591.132 ± 15.293 ns** | 19.738 ± 0.175 µs | 3.236 ± 0.085 µs | 579.061 ± 2.314 µs |
| generated/large-array | 19.372 ± 0.146 ms | 74.900 ± 0.715 ms | 8.060 ± 0.285 ms | **2.657 ± 0.012 ms** | 88.017 ± 0.685 ms | 9.490 ± 0.344 ms | 284.336 ± 4.662 ms |
| generated/many-defs | 1.860 ± 0.019 ms | 5.158 ± 0.156 ms | 657.134 ± 12.282 µs | **114.233 ± 0.234 µs** | 328.079 ± 3.133 ms | 802.129 ± 11.793 µs | 41.168 ± 0.237 ms |
| generated/wide-object | 177.798 ± 0.474 µs | 450.004 ± 6.880 µs | 113.575 ± 1.898 µs | 160.811 ± 4.674 µs | 499.732 ± 5.091 µs | **56.541 ± 0.280 µs** | 1.303 ± 0.008 ms |
| large/api-catalog | 590.206 ± 3.599 ns | 2.305 ± 0.033 µs | 253.045 ± 3.608 ns | **97.650 ± 0.629 ns** | 29.122 ± 0.176 µs | 444.491 ± 6.203 ns | 6.824 ± 0.024 µs |
| large/circleci | 2.361 ± 0.024 µs | 12.613 ± 0.075 µs | 486.082 ± 4.649 ns | **206.372 ± 0.714 ns** | 170.546 ± 0.528 µs | 1.048 ± 0.016 µs | 20.766 ± 0.053 µs |
| massive/data-warehouse | 73.026 ± 0.085 µs | 57.499 ± 0.487 ms | 25.244 ± 0.214 µs | **7.431 ± 0.046 µs** | 26.903 ± 0.142 ms | 46.684 ± 0.271 µs | 190.658 ± 0.514 ms |
| massive/k8s-deployment | 541.933 ± 1.269 ns | 2.630 ± 0.009 µs | 219.932 ± 0.519 ns | **57.127 ± 0.424 ns** | 3.137 ± 0.012 µs | 518.551 ± 4.429 ns | 8.195 ± 0.021 µs |
| medium/package-json | 558.826 ± 0.871 ns | 2.882 ± 0.008 µs | **241.070 ± 1.139 ns** | 782.785 ± 4.967 ns | — | 511.614 ± 0.928 ns | 5.866 ± 0.018 µs |
| medium/user-profile | 877.956 ± 5.868 ns | 3.564 ± 0.042 µs | 389.980 ± 3.324 ns | **176.813 ± 1.512 ns** | 13.565 ± 0.022 µs | 518.715 ± 6.490 ns | 8.935 ± 0.007 µs |
| small/config | 231.878 ± 0.648 ns | 949.084 ± 11.356 ns | 118.683 ± 0.488 ns | **65.257 ± 0.680 ns** | 9.418 ± 0.035 µs | 146.589 ± 2.405 ns | 1.918 ± 0.002 µs |
| small/prettierrc | 745.218 ± 3.126 ns | 5.073 ± 0.060 µs | 182.214 ± 2.450 ns | **54.211 ± 0.210 ns** | 8.785 ± 0.030 µs | 289.023 ± 1.810 ns | 7.616 ± 0.021 µs |

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
| generated/deep-nesting | 8.077 ± 0.013 µs | 34.354 ± 0.087 µs | **2.926 ± 0.040 µs** | 14.282 ± 0.031 µs | 24.351 ± 0.163 µs | 5.398 ± 0.022 µs | 581.039 ± 0.399 µs |
| generated/large-array | 19.075 ± 0.066 ms | 73.872 ± 0.502 ms | 7.733 ± 0.138 ms | **5.332 ± 0.016 ms** | 87.153 ± 0.400 ms | 9.042 ± 0.087 ms | 269.728 ± 1.873 ms |
| generated/many-defs | 1.849 ± 0.009 ms | 4.816 ± 0.018 ms | 657.765 ± 1.976 µs | **142.839 ± 0.433 µs** | 326.546 ± 2.448 ms | 793.386 ± 4.649 µs | 40.123 ± 0.055 ms |
| generated/wide-object | 178.499 ± 0.665 µs | 454.866 ± 9.358 µs | 115.118 ± 1.820 µs | 161.739 ± 3.980 µs | 494.608 ± 1.070 µs | **56.939 ± 0.174 µs** | 1.293 ± 0.018 ms |
| large/api-catalog | 686.212 ± 2.649 ns | 2.473 ± 0.019 µs | 406.755 ± 2.631 ns | **319.891 ± 4.901 ns** | 21.234 ± 0.054 µs | 1.068 ± 0.005 µs | 4.444 ± 0.005 µs |
| large/circleci | 132.206 ± 0.538 ns | 595.035 ± 3.745 ns | 135.772 ± 0.839 ns | 153.513 ± 0.746 ns | 573.412 ± 5.085 ns | 280.857 ± 1.889 ns | 474.438 ± 4.767 ns |
| massive/data-warehouse | 187.863 ± 0.511 µs | 389.974 ± 1.309 µs | 145.924 ± 0.628 µs | **108.349 ± 0.351 µs** | 45.770 ± 0.371 ms | 281.600 ± 0.616 µs | 912.743 ± 3.147 µs |
| massive/k8s-deployment | 187.486 ± 0.716 ns | 744.850 ± 6.249 ns | 135.404 ± 0.287 ns | 132.304 ± 1.150 ns | 247.421 ± 0.708 µs | 247.927 ± 0.613 ns | 935.771 ± 2.682 ns |
| medium/package-json | 182.529 ± 0.528 ns | 787.297 ± 6.481 ns | **156.877 ± 0.877 ns** | 388.549 ± 2.788 ns | — | 251.599 ± 2.013 ns | 875.492 ± 2.103 ns |
| medium/user-profile | 390.787 ± 3.874 ns | 1.806 ± 0.014 µs | 335.885 ± 9.107 ns | **292.123 ± 1.756 ns** | 8.443 ± 0.047 µs | 570.521 ± 9.553 ns | 2.508 ± 0.028 µs |
| small/config | 301.434 ± 3.677 ns | 1.493 ± 0.005 µs | 275.650 ± 5.931 ns | **231.763 ± 1.170 ns** | 9.888 ± 0.075 µs | 568.952 ± 10.433 ns | 1.720 ± 0.006 µs |
| small/prettierrc | **555.755 ± 4.315 ns** | 2.548 ± 0.011 µs | 947.856 ± 7.603 ns | 844.053 ± 15.671 ns | 5.532 ± 0.058 µs | 1.716 ± 0.038 µs | 2.075 ± 0.019 µs |

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
| *startup* | 242.431 ± 2.322 ms | 164.488 ± 1.814 ms | 2.140 ± 0.155 ms | 285.696 ± 2.379 ms | 91.026 ± 0.829 ms |
| generated/deep-nesting | 260.066 ± 1.865 ms | 179.647 ± 2.068 ms | **6.577 ± 0.070 ms** | 314.466 ± 2.218 ms | 117.172 ± 1.986 ms |
| generated/large-array | 250.465 ± 1.777 ms | 171.629 ± 2.436 ms | **2.644 ± 0.083 ms** | 294.505 ± 3.275 ms | 95.664 ± 1.303 ms |
| generated/many-defs | 509.401 ± 5.371 ms | 268.472 ± 104.659 ms † | **30.240 ± 0.206 ms** | 479.051 ± 3.865 ms | 463.482 ± 2.251 ms |
| generated/wide-object | 675.931 ± 10.686 ms | 309.166 ± 3.324 ms | **32.919 ± 3.363 ms †** | 515.288 ± 15.220 ms | 568.085 ± 127.417 ms † |
| large/api-catalog | 267.820 ± 7.526 ms | 181.951 ± 3.702 ms | **3.683 ± 0.297 ms** | 315.302 ± 4.026 ms | 105.874 ± 2.087 ms |
| large/circleci | 353.313 ± 13.086 ms | 223.471 ± 4.881 ms | **12.983 ± 1.434 ms †** | — | — |
| massive/data-warehouse | 1.169 ± 0.021 s | 348.288 ± 3.354 ms | **47.440 ± 1.205 ms** | 648.050 ± 7.092 ms | 842.614 ± 6.196 ms |
| massive/k8s-deployment | 509.022 ± 9.610 ms | 285.286 ± 6.933 ms | **23.912 ± 0.657 ms** | 445.804 ± 11.336 ms | 95.731 ± 2.377 ms |
| medium/package-json | — | — | 108.632 ± 5.082 ms | — | — |
| medium/user-profile | 253.570 ± 2.099 ms | 171.717 ± 2.339 ms | **2.812 ± 0.099 ms** | 301.043 ± 1.905 ms | 96.776 ± 0.969 ms |
| small/config | 247.055 ± 1.722 ms | 167.810 ± 0.940 ms | **2.606 ± 0.153 ms** | 297.350 ± 3.424 ms | 92.728 ± 1.137 ms |
| small/prettierrc | 294.973 ± 1.332 ms | 183.392 ± 0.824 ms | **3.791 ± 0.048 ms** | 316.718 ± 3.755 ms | — |

### codegen — peak RSS

| Fixture | datamodel-code-generator | json-schema-to-typescript | jsonschemars | quicktype | typify |
|---|---|---|---|---|---|
| *startup* | 58.47 ± 0.23 MiB | 73.73 ± 0.14 MiB | 3.72 MiB | 126.34 ± 0.13 MiB | 31.64 ± 0.01 MiB |
| generated/deep-nesting | 61.34 ± 0.26 MiB | 78.44 ± 0.08 MiB | **7.73 ± 0.01 MiB** | 152.17 ± 0.38 MiB | 37.88 ± 0.02 MiB |
| generated/large-array | 61.02 MiB | 76.44 ± 0.02 MiB | **4.67 MiB** | 130.19 ± 0.04 MiB | 33.14 ± 0.01 MiB |
| generated/many-defs | 82.92 ± 0.17 MiB | 122.89 ± 3.43 MiB | **23.39 ± 0.00 MiB** | 205.42 ± 0.16 MiB | 97.95 ± 0.13 MiB |
| generated/wide-object | 97.92 MiB | 169.17 ± 0.46 MiB | **24.06 ± 0.03 MiB** | 221.70 ± 2.17 MiB | 118.11 MiB |
| large/api-catalog | 62.91 ± 0.12 MiB | 77.25 MiB | **5.42 ± 0.02 MiB** | 143.64 ± 0.13 MiB | 34.77 ± 0.11 MiB |
| large/circleci | 70.31 MiB | 92.48 ± 0.23 MiB | **11.50 ± 0.11 MiB** | — | — |
| massive/data-warehouse | 176.02 ± 0.40 MiB | 179.77 MiB | **31.73 ± 0.26 MiB** | 226.61 ± 0.01 MiB | 176.84 ± 0.08 MiB |
| massive/k8s-deployment | 87.69 ± 0.32 MiB | 131.88 ± 0.53 MiB | **18.52 MiB** | 202.22 ± 0.26 MiB | 31.67 MiB |
| medium/package-json | — | — | 44.89 MiB | — | — |
| medium/user-profile | 60.88 MiB | 76.33 MiB | **4.92 ± 0.01 MiB** | 132.62 ± 0.27 MiB | 33.42 ± 0.03 MiB |
| small/config | 59.61 MiB | 74.61 ± 0.45 MiB | **4.33 ± 0.02 MiB** | 127.77 ± 0.02 MiB | 32.78 ± 0.01 MiB |
| small/prettierrc | 66.73 ± 0.22 MiB | 78.14 ± 0.57 MiB | **5.72 ± 0.01 MiB** | 148.47 ± 0.42 MiB | — |

### validate (valid) — time

| Fixture | ajv | blaze | boon | fastjsonschema | jsonschema-cli | jsonschemars | python-jsonschema |
|---|---|---|---|---|---|---|---|
| *startup* | 121.262 ± 1.179 ms | 3.779 ± 0.085 ms | 3.329 ± 0.045 ms | 28.360 ± 0.274 ms | 4.332 ± 0.164 ms | 1.909 ± 0.057 ms | 68.794 ± 0.636 ms |
| generated/deep-nesting | 146.268 ± 1.088 ms | 15.485 ± 0.319 ms | 4.699 ± 0.087 ms | 103.184 ± 0.695 ms | 5.007 ± 0.130 ms | **2.305 ± 0.052 ms** | 83.298 ± 0.772 ms |
| generated/large-array | 170.418 ± 1.746 ms | 104.735 ± 0.883 ms | 61.709 ± 0.858 ms | 156.620 ± 1.102 ms | 48.085 ± 0.431 ms | 47.533 ± 0.554 ms | 1.436 ± 0.010 s |
| generated/many-defs | 268.444 ± 4.727 ms | 139.007 ± 16.924 ms † | 32.117 ± 4.336 ms † | 488.142 ± 110.089 ms † | 18.593 ± 2.967 ms † | **10.971 ± 2.318 ms †** | 442.977 ± 4.941 ms |
| generated/wide-object | — | 63.743 ± 0.968 ms | 29.956 ± 0.254 ms | 340.739 ± 4.575 ms | 9.453 ± 0.395 ms | **4.593 ± 0.288 ms** | 316.914 ± 2.902 ms |
| large/api-catalog | 140.924 ± 102.333 ms † | 5.923 ± 0.657 ms † | 3.325 ± 0.717 ms † | 46.359 ± 3.169 ms | 5.185 ± 0.177 ms | 2.367 ± 0.275 ms † | 81.946 ± 3.751 ms |
| large/circleci | 184.434 ± 7.834 ms | 32.206 ± 1.424 ms | 8.727 ± 1.904 ms † | 353.822 ± 5.305 ms | 7.307 ± 0.556 ms | **3.122 ± 0.422 ms †** | 97.732 ± 7.124 ms |
| massive/data-warehouse | 593.906 ± 11.921 ms | 272.183 ± 6.472 ms | 53.921 ± 1.210 ms | 1.329 ± 0.020 s | 13.738 ± 0.456 ms | **6.225 ± 0.480 ms** | 490.255 ± 8.497 ms |
| massive/k8s-deployment | — | — | 13.895 ± 0.562 ms | 403.786 ± 5.757 ms | 231.379 ± 59.412 ms † | **3.402 ± 0.497 ms †** | 242.625 ± 5.109 ms |
| medium/package-json | 892.617 ± 17.797 ms | 399.600 ± 5.753 ms | 84.204 ± 1.374 ms | 927.582 ± 12.367 ms | — | **8.823 ± 0.615 ms** | 73.328 ± 8.437 ms † |
| medium/user-profile | 129.808 ± 0.684 ms | 4.581 ± 0.150 ms | 3.472 ± 0.063 ms | 32.374 ± 0.391 ms | 4.664 ± 0.395 ms | **2.080 ± 0.052 ms** | 71.389 ± 0.575 ms |
| small/config | 128.728 ± 1.198 ms | 3.958 ± 0.143 ms | 3.385 ± 0.053 ms | 28.911 ± 0.465 ms | 4.622 ± 0.195 ms | **2.026 ± 0.034 ms** | 68.882 ± 0.934 ms |
| small/prettierrc | 133.242 ± 0.954 ms | 6.741 ± 0.130 ms | 3.735 ± 0.200 ms | 42.802 ± 0.205 ms | 4.672 ± 0.136 ms | **2.192 ± 0.039 ms** | 70.376 ± 0.964 ms |

### validate (valid) — peak RSS

| Fixture | ajv | blaze | boon | fastjsonschema | jsonschema-cli | jsonschemars | python-jsonschema |
|---|---|---|---|---|---|---|---|
| *startup* | 53.27 ± 0.05 MiB | 8.50 MiB | 5.67 ± 0.01 MiB | 17.16 MiB | 8.44 MiB | 3.48 MiB | 24.72 MiB |
| generated/deep-nesting | 62.58 MiB | 18.08 MiB | 7.03 ± 0.02 MiB | 87.36 MiB | 10.05 ± 0.01 MiB | **4.09 ± 0.01 MiB** | 25.08 MiB |
| generated/large-array | 84.02 ± 0.17 MiB | 300.80 MiB | **55.91 ± 0.01 MiB** | 60.20 ± 0.03 MiB | 64.89 ± 0.01 MiB | 59.66 ± 0.01 MiB | 65.56 ± 0.02 MiB |
| generated/many-defs | 82.42 MiB | 84.83 MiB | 20.86 ± 0.09 MiB | 328.06 ± 0.28 MiB | 21.92 ± 0.01 MiB | **13.56 MiB** | 28.30 ± 0.07 MiB |
| generated/wide-object | — | 26.86 MiB | 15.23 ± 0.03 MiB | 282.02 ± 0.73 MiB | 15.48 ± 0.01 MiB | **7.41 ± 0.04 MiB** | 26.25 ± 0.11 MiB |
| large/api-catalog | 55.92 ± 0.24 MiB | 9.55 MiB | 6.00 ± 0.01 MiB | 28.47 MiB | 10.55 ± 0.01 MiB | **3.97 ± 0.02 MiB** | 25.11 ± 0.06 MiB |
| large/circleci | 75.89 MiB | 19.53 MiB | 9.59 ± 0.16 MiB | 271.61 ± 0.57 MiB | 13.42 ± 0.02 MiB | **5.55 ± 0.02 MiB** | 25.72 ± 0.03 MiB |
| massive/data-warehouse | 152.56 ± 0.43 MiB | 297.67 MiB | 23.91 ± 0.44 MiB | 972.41 ± 0.06 MiB | 21.42 MiB | **9.53 MiB** | 27.70 ± 0.02 MiB |
| massive/k8s-deployment | — | — | 13.39 MiB | 319.52 ± 0.20 MiB | 20.47 ± 0.01 MiB | **6.00 MiB** | 26.73 ± 0.01 MiB |
| medium/package-json | 362.97 MiB | 68.64 MiB | 28.95 ± 0.04 MiB | 662.39 MiB | — | **11.89 ± 0.09 MiB** | 28.44 MiB |
| medium/user-profile | 54.86 ± 0.10 MiB | 9.22 MiB | 5.89 ± 0.03 MiB | 20.47 ± 0.13 MiB | 10.36 MiB | **3.80 ± 0.01 MiB** | 24.75 ± 0.06 MiB |
| small/config | 54.28 ± 0.01 MiB | 8.91 MiB | 5.81 MiB | 18.00 MiB | 10.17 ± 0.01 MiB | **3.73 ± 0.01 MiB** | 24.61 ± 0.22 MiB |
| small/prettierrc | 58.14 ± 0.03 MiB | 10.11 ± 0.01 MiB | 5.91 MiB | 30.06 ± 0.07 MiB | 9.22 MiB | **3.89 ± 0.00 MiB** | 24.69 ± 0.01 MiB |

### validate (invalid) — time

| Fixture | ajv | blaze | boon | fastjsonschema | jsonschema-cli | jsonschemars | python-jsonschema |
|---|---|---|---|---|---|---|---|
| *startup* | 121.290 ± 0.983 ms | 3.856 ± 0.084 ms | 3.356 ± 0.055 ms | 28.161 ± 0.665 ms | 4.296 ± 0.124 ms | 2.082 ± 0.135 ms | 68.154 ± 0.439 ms |
| generated/deep-nesting | 146.179 ± 0.932 ms | 38.064 ± 0.269 ms | 4.700 ± 0.103 ms | 103.042 ± 0.845 ms | 4.934 ± 0.093 ms | **2.486 ± 0.180 ms** | 82.733 ± 0.603 ms |
| generated/large-array | 171.520 ± 2.161 ms | 122.802 ± 1.333 ms | 61.752 ± 0.448 ms | 157.941 ± 7.203 ms | 47.558 ± 0.455 ms | 47.533 ± 0.451 ms | 1.477 ± 0.036 s |
| generated/many-defs | 267.855 ± 7.715 ms | 136.215 ± 3.163 ms | 33.103 ± 6.722 ms † | 428.185 ± 16.390 ms | 26.283 ± 8.596 ms † | **11.154 ± 1.416 ms †** | 444.784 ± 4.921 ms |
| generated/wide-object | — | 63.176 ± 0.570 ms | 30.093 ± 0.251 ms | 350.272 ± 7.201 ms | 9.440 ± 0.338 ms | **4.686 ± 0.399 ms** | 321.324 ± 4.471 ms |
| large/api-catalog | 226.590 ± 55.864 ms † | 6.231 ± 0.436 ms | 3.713 ± 0.405 ms † | 47.222 ± 8.886 ms † | 4.750 ± 0.604 ms † | **2.468 ± 0.202 ms** | 84.657 ± 7.627 ms |
| large/circleci | 184.762 ± 25.174 ms † | 32.327 ± 1.175 ms | 8.091 ± 0.431 ms | 353.623 ± 4.100 ms | 8.744 ± 1.194 ms † | **3.515 ± 0.214 ms** | 85.932 ± 2.618 ms |
| massive/data-warehouse | 526.183 ± 11.859 ms | 335.322 ± 5.000 ms | 55.111 ± 1.050 ms | 1.334 ± 0.014 s | 13.968 ± 0.574 ms | **6.211 ± 0.371 ms** | 500.214 ± 8.285 ms |
| massive/k8s-deployment | — | — | 13.073 ± 0.656 ms | 399.219 ± 8.288 ms | 223.495 ± 54.419 ms † | **3.888 ± 0.207 ms** | 240.980 ± 8.395 ms |
| medium/package-json | 922.261 ± 31.770 ms | 404.018 ± 8.824 ms | 82.193 ± 2.140 ms | 920.859 ± 4.502 ms | — | **8.690 ± 0.857 ms** | 84.907 ± 6.642 ms |
| medium/user-profile | 129.942 ± 1.029 ms | 4.720 ± 0.223 ms | 3.433 ± 0.097 ms | 32.828 ± 0.725 ms | 4.559 ± 0.104 ms | **2.134 ± 0.101 ms** | 71.567 ± 0.655 ms |
| small/config | 129.091 ± 1.103 ms | 4.205 ± 0.248 ms | 3.399 ± 0.122 ms | 30.389 ± 0.216 ms | 4.551 ± 0.101 ms | **2.179 ± 0.144 ms** | 68.914 ± 0.439 ms |
| small/prettierrc | 132.697 ± 1.028 ms | 7.323 ± 0.199 ms | 3.785 ± 0.134 ms | 42.582 ± 0.264 ms | 4.736 ± 0.199 ms | **2.266 ± 0.243 ms †** | 70.430 ± 0.280 ms |

### validate (invalid) — peak RSS

| Fixture | ajv | blaze | boon | fastjsonschema | jsonschema-cli | jsonschemars | python-jsonschema |
|---|---|---|---|---|---|---|---|
| *startup* | 53.47 ± 0.12 MiB | 8.67 MiB | 5.69 MiB | 17.22 ± 0.01 MiB | 8.58 MiB | 3.55 MiB | 25.20 MiB |
| generated/deep-nesting | 62.56 ± 0.03 MiB | 18.25 ± 0.00 MiB | 7.05 ± 0.00 MiB | 87.33 ± 0.63 MiB | 10.16 ± 0.01 MiB | **4.19 MiB** | 25.06 ± 0.03 MiB |
| generated/large-array | 84.30 ± 0.06 MiB | 326.53 MiB | **55.92 MiB** | 60.27 ± 0.06 MiB | 65.02 ± 0.01 MiB | 59.70 ± 0.01 MiB | 65.88 ± 0.13 MiB |
| generated/many-defs | 81.61 ± 0.02 MiB | 85.44 MiB | 21.03 MiB | 328.78 MiB | 22.11 MiB | **13.61 ± 0.02 MiB** | 28.27 MiB |
| generated/wide-object | — | 27.56 ± 0.00 MiB | 15.25 MiB | 283.52 MiB | 15.61 ± 0.02 MiB | **7.42 MiB** | 26.31 ± 0.10 MiB |
| large/api-catalog | 64.34 ± 4.00 MiB | 9.69 MiB | 6.02 MiB | 28.03 MiB | 10.70 ± 0.01 MiB | **4.02 MiB** | 25.50 MiB |
| large/circleci | 75.75 ± 1.22 MiB | 19.48 MiB | 9.22 ± 0.15 MiB | 271.58 ± 0.52 MiB | 13.48 ± 0.01 MiB | **5.66 MiB** | 25.72 ± 0.17 MiB |
| massive/data-warehouse | 134.67 MiB | 68.66 MiB | 24.73 MiB | 974.53 MiB | 21.34 ± 0.05 MiB | **9.70 MiB** | 28.50 MiB |
| massive/k8s-deployment | — | — | 13.27 ± 0.67 MiB | 320.59 ± 0.52 MiB | 20.53 ± 0.02 MiB | **5.98 ± 0.01 MiB** | 26.73 ± 0.01 MiB |
| medium/package-json | 363.25 ± 0.54 MiB | 68.72 MiB | 29.03 ± 0.18 MiB | 658.30 ± 0.02 MiB | — | **11.89 ± 0.08 MiB** | 28.52 ± 0.08 MiB |
| medium/user-profile | 54.77 ± 0.00 MiB | 9.31 MiB | 5.88 ± 0.01 MiB | 20.48 MiB | 10.50 MiB | **3.83 MiB** | 24.75 MiB |
| small/config | 54.45 ± 0.06 MiB | 9.06 ± 0.01 MiB | 5.84 ± 0.01 MiB | 18.03 ± 0.02 MiB | 10.31 ± 0.01 MiB | **3.80 MiB** | 24.73 ± 0.01 MiB |
| small/prettierrc | 58.05 ± 0.14 MiB | 10.19 MiB | 5.97 ± 0.01 MiB | 30.06 ± 0.02 MiB | 9.38 ± 0.01 MiB | **3.97 ± 0.02 MiB** | 24.72 ± 0.03 MiB |

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
