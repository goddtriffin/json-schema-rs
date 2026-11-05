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
| in-process | codegen | time | schemafy | 7 / 11 |
| in-process | codegen | peak heap | jsonschema_code_generator, schemafy (tied) | 5 each / 11 |
| in-process | schema compile | time | jsonschema-valid | 11 / 12 |
| in-process | schema compile | peak heap | jsonschema-valid | 11 / 12 |
| in-process | validate (valid) | time | jsonschema-macros | 9 / 12 |
| in-process | validate (valid) | peak heap | jsonschemars | 11 / 12 |
| in-process | validate (invalid) | time | jsonschema-macros | 5 / 12 |
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
| generated/deep-nesting | 862.931 ± 20.853 µs | 5.192 ± 1.978 ms † | **443.549 ± 8.864 µs** | 11.173 ± 0.145 ms |
| generated/large-array | 83.956 ± 28.255 µs † | 146.374 ± 1.116 µs | **7.829 ± 6.312 µs †** | 1.141 ± 0.528 ms † |
| generated/many-defs | 6.941 ± 0.199 ms | 22.949 ± 0.317 ms | **210.187 ± 2.141 µs** | 136.233 ± 2.486 ms |
| generated/wide-object | 9.011 ± 0.094 ms | 20.121 ± 0.374 ms | 9.200 ± 0.255 ms | 165.764 ± 3.309 ms |
| large/api-catalog | 303.638 ± 95.413 µs † | 509.930 ± 8.810 µs | **26.439 ± 0.171 µs** | 3.522 ± 0.114 ms |
| large/circleci | — | 9.320 ± 2.211 ms † | **3.039 ± 0.832 ms †** | — |
| massive/data-warehouse | **12.428 ± 0.065 ms** | 56.945 ± 20.952 ms † | — | 276.713 ± 15.586 ms |
| massive/k8s-deployment | — | 16.025 ± 0.326 ms | 12.699 ± 3.878 ms † | **1.865 ± 0.275 ms †** |
| medium/package-json | — | 62.621 ± 13.502 ms † | — | — |
| medium/user-profile | 101.826 ± 4.638 µs | 195.721 ± 33.093 µs † | **33.249 ± 0.334 µs** | 1.506 ± 0.064 ms |
| small/config | 40.672 ± 3.248 µs | 70.860 ± 2.931 µs | **24.330 ± 8.612 µs †** | 701.825 ± 426.559 µs † |
| small/prettierrc | **250.912 ± 5.577 µs** | 908.993 ± 19.970 µs | 834.359 ± 11.385 µs | — |

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
| generated/deep-nesting | 1.305 ± 0.459 ms † | 278.951 ± 1.206 µs | 222.291 ± 85.709 µs † | **66.913 ± 5.939 µs** | 102.964 ± 0.314 µs | 1.668 ± 0.030 ms |
| generated/large-array | 50.542 ± 12.562 µs † | 28.916 ± 0.104 µs | 28.197 ± 6.232 µs † | **4.369 ± 0.081 µs** | 11.142 ± 0.117 µs | 37.857 ± 1.356 µs |
| generated/many-defs | 15.100 ± 0.214 ms | 3.572 ± 0.018 ms | 3.244 ± 0.145 ms | **676.703 ± 3.465 µs** | 1.782 ± 0.046 ms | 4.805 ± 0.060 ms |
| generated/wide-object | 25.239 ± 0.334 ms | 6.247 ± 0.025 ms | 2.767 ± 0.028 ms | **760.598 ± 13.394 µs** | 1.408 ± 0.006 ms | 3.729 ± 0.023 ms |
| large/api-catalog | 190.132 ± 1.504 µs | 101.440 ± 0.314 µs | 110.172 ± 0.344 µs | **15.655 ± 0.056 µs** | 46.414 ± 0.216 µs | 147.182 ± 0.706 µs |
| large/circleci | 3.108 ± 0.032 ms | 1.100 ± 0.011 ms | 1.119 ± 0.012 ms | **225.031 ± 1.362 µs** | 549.667 ± 3.626 µs | 4.508 ± 0.071 ms |
| massive/data-warehouse | 48.477 ± 0.487 ms | 7.625 ± 0.071 ms | 7.178 ± 0.113 ms | **1.271 ± 0.010 ms** | 2.888 ± 0.033 ms | 11.270 ± 0.300 ms |
| massive/k8s-deployment | 8.031 ± 0.077 ms | 2.545 ± 0.021 ms | 1.731 ± 0.015 ms | **536.220 ± 1.706 µs** | 811.090 ± 5.304 µs | 8.165 ± 0.100 ms |
| medium/package-json | 76.581 ± 0.779 ms | 10.963 ± 0.118 ms | 17.249 ± 0.531 ms | — | **6.627 ± 0.100 ms** | 20.057 ± 0.379 ms |
| medium/user-profile | 71.465 ± 0.690 µs | 38.792 ± 0.183 µs | 42.210 ± 0.204 µs | **6.391 ± 0.033 µs** | 17.526 ± 0.067 µs | 49.648 ± 0.253 µs |
| small/config | 28.757 ± 0.109 µs | 11.648 ± 0.056 µs | 20.215 ± 0.084 µs | **2.563 ± 0.017 µs** | 7.959 ± 0.050 µs | 22.057 ± 0.087 µs |
| small/prettierrc | 218.223 ± 0.830 µs | 169.530 ± 1.038 µs | 117.703 ± 0.517 µs | **25.966 ± 0.086 µs** | 60.541 ± 2.616 µs | 248.111 ± 0.945 µs |

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
| generated/deep-nesting | 6.659 ± 0.039 µs | 29.870 ± 0.121 µs | 2.489 ± 0.050 µs | **641.489 ± 15.441 ns** | 19.926 ± 0.146 µs | 3.265 ± 0.079 µs | 587.859 ± 2.864 µs |
| generated/large-array | 19.689 ± 0.147 ms | 76.149 ± 0.359 ms | 8.686 ± 0.216 ms | **2.707 ± 1.311 ms †** | 89.251 ± 0.390 ms | 10.830 ± 0.260 ms | 313.673 ± 8.606 ms |
| generated/many-defs | 1.933 ± 0.027 ms | 5.175 ± 0.066 ms | 678.377 ± 10.866 µs | **251.381 ± 153.844 µs †** | 338.536 ± 2.571 ms | 822.956 ± 20.285 µs | 42.411 ± 0.372 ms |
| generated/wide-object | 179.934 ± 1.365 µs | 463.691 ± 5.002 µs | 117.978 ± 2.392 µs | 192.709 ± 40.247 µs † | 506.272 ± 3.441 µs | **57.315 ± 0.645 µs** | 1.335 ± 0.011 ms |
| large/api-catalog | 605.697 ± 4.208 ns | 2.326 ± 0.017 µs | 258.771 ± 4.304 ns | **108.585 ± 15.870 ns †** | 29.330 ± 0.125 µs | 453.371 ± 9.628 ns | 6.835 ± 0.046 µs |
| large/circleci | 2.345 ± 0.024 µs | 12.879 ± 0.091 µs | 509.364 ± 5.608 ns | **223.933 ± 84.404 ns †** | 176.136 ± 0.884 µs | 1.089 ± 0.016 µs | 21.315 ± 0.135 µs |
| massive/data-warehouse | 75.354 ± 0.411 µs | 58.978 ± 0.571 ms | 25.643 ± 0.226 µs | **7.894 ± 0.217 µs** | 27.874 ± 0.339 ms | 48.614 ± 0.581 µs | 205.076 ± 4.321 ms |
| massive/k8s-deployment | 551.906 ± 4.058 ns | 2.706 ± 0.022 µs | 227.032 ± 2.888 ns | **103.419 ± 53.245 ns †** | 3.230 ± 0.016 µs | 534.344 ± 8.622 ns | 8.438 ± 0.045 µs |
| medium/package-json | 571.265 ± 2.981 ns | 2.989 ± 0.019 µs | **246.658 ± 3.271 ns** | 1.144 ± 0.428 µs † | — | 513.238 ± 2.664 ns | 5.941 ± 0.047 µs |
| medium/user-profile | 901.768 ± 6.380 ns | 3.619 ± 0.023 µs | 406.232 ± 7.244 ns | 323.472 ± 154.900 ns † | 13.854 ± 0.079 µs | 530.457 ± 8.025 ns | 9.118 ± 0.048 µs |
| small/config | 240.770 ± 1.458 ns | 967.669 ± 8.310 ns | 124.288 ± 1.262 ns | **69.702 ± 2.723 ns** | 9.637 ± 0.054 µs | 156.463 ± 4.177 ns | 1.955 ± 0.009 µs |
| small/prettierrc | 768.488 ± 5.471 ns | 5.208 ± 0.054 µs | 190.154 ± 3.115 ns | **57.338 ± 2.233 ns** | 8.966 ± 0.072 µs | 298.572 ± 5.251 ns | 7.739 ± 0.051 µs |

### validate (valid) — peak heap

| Fixture | boon | corvus-json-schema | jsonschema | jsonschema-macros | jsonschema-valid | jsonschemars | valico |
|---|---|---|---|---|---|---|---|
| generated/deep-nesting | 2.25 KiB | 23.40 KiB | 0.03 KiB | 0.03 KiB | 13.29 KiB | **0.00 KiB** | 37.75 KiB |
| generated/large-array | 0.19 KiB | 1.49 KiB | 0.03 KiB | 0.03 KiB | 2.05 KiB | **0.00 KiB** | 39.97 MiB |
| generated/many-defs | 0.56 KiB | 2.29 KiB | 0.03 KiB | 0.03 KiB | 10.12 KiB | **0.00 KiB** | 3.34 MiB |
| generated/wide-object | 0.19 KiB | 1.19 KiB | 0.03 KiB | 0.03 KiB | 0.76 KiB | **0.00 KiB** | 236.68 KiB |
| large/api-catalog | 0.19 KiB | 1.48 KiB | 0.03 KiB | 0.03 KiB | 10.41 KiB | **0.00 KiB** | 1.44 KiB |
| large/circleci | 2.58 KiB | 3.26 KiB | 0.03 KiB | 0.03 KiB | 328.25 KiB | 0.22 KiB | 3.37 KiB |
| massive/data-warehouse | 0.19 KiB | 1.48 KiB | 0.03 KiB | 0.03 KiB | 12.55 KiB | **0.00 KiB** | 155.85 KiB |
| massive/k8s-deployment | 0.19 KiB | 1.59 KiB | 0.03 KiB | 0.03 KiB | 2.07 KiB | **0.00 KiB** | 1.62 KiB |
| medium/package-json | 0.19 KiB | 1.23 KiB | 0.03 KiB | 0.03 KiB | — | **0.00 KiB** | 1.20 KiB |
| medium/user-profile | 0.19 KiB | 1.23 KiB | 0.03 KiB | 0.03 KiB | 8.74 KiB | **0.00 KiB** | 1.87 KiB |
| small/config | 0.19 KiB | 1.18 KiB | 0.03 KiB | 0.03 KiB | 10.18 KiB | **0.00 KiB** | 0.60 KiB |
| small/prettierrc | 0.77 KiB | 1.49 KiB | 0.03 KiB | 0.03 KiB | 2.05 KiB | **0.00 KiB** | 1.61 KiB |

### validate (invalid) — time

| Fixture | boon | corvus-json-schema | jsonschema | jsonschema-macros | jsonschema-valid | jsonschemars | valico |
|---|---|---|---|---|---|---|---|
| generated/deep-nesting | 7.933 ± 0.044 µs | 35.399 ± 0.334 µs | **2.986 ± 0.021 µs** | 15.311 ± 0.297 µs | 24.618 ± 0.140 µs | 5.500 ± 0.059 µs | 593.159 ± 2.757 µs |
| generated/large-array | 19.535 ± 0.126 ms | 76.176 ± 0.575 ms | 8.729 ± 0.263 ms | **5.388 ± 0.071 ms** | 89.621 ± 0.742 ms | 10.611 ± 0.331 ms | 297.539 ± 8.153 ms |
| generated/many-defs | 2.000 ± 0.045 ms | 5.170 ± 0.086 ms | 681.764 ± 14.505 µs | **148.560 ± 1.925 µs** | 337.143 ± 4.193 ms | 826.623 ± 15.789 µs | 43.032 ± 0.567 ms |
| generated/wide-object | 181.500 ± 1.430 µs | 461.968 ± 2.839 µs | 117.791 ± 3.819 µs | 170.675 ± 9.762 µs | 505.389 ± 1.877 µs | **58.280 ± 0.630 µs** | 1.329 ± 0.014 ms |
| large/api-catalog | 704.954 ± 4.932 ns | 2.572 ± 0.018 µs | 426.698 ± 5.039 ns | **334.616 ± 5.057 ns** | 21.742 ± 0.194 µs | 1.132 ± 0.026 µs | 4.526 ± 0.023 µs |
| large/circleci | **135.577 ± 0.727 ns** | 616.615 ± 5.656 ns | 141.774 ± 1.309 ns | 158.101 ± 1.067 ns | 581.419 ± 4.877 ns | 294.327 ± 4.784 ns | 481.192 ± 2.744 ns |
| massive/data-warehouse | 196.341 ± 1.368 µs | 402.201 ± 3.087 µs | 149.029 ± 1.295 µs | 170.434 ± 91.495 µs † | 47.452 ± 0.343 ms | 295.526 ± 3.797 µs | 956.766 ± 16.093 µs |
| massive/k8s-deployment | 186.864 ± 1.807 ns | 754.106 ± 7.312 ns | 142.638 ± 1.886 ns | 160.234 ± 118.421 ns † | 251.052 ± 1.360 µs | 264.566 ± 3.168 ns | 947.395 ± 5.629 ns |
| medium/package-json | 184.202 ± 1.167 ns | 823.854 ± 7.329 ns | **163.599 ± 1.980 ns** | 698.033 ± 213.137 ns † | — | 263.720 ± 2.925 ns | 891.013 ± 2.808 ns |
| medium/user-profile | 398.879 ± 5.829 ns | 1.898 ± 0.024 µs | 339.085 ± 3.983 ns | **305.558 ± 17.529 ns** | 8.531 ± 0.062 µs | 582.240 ± 6.351 ns | 2.524 ± 0.011 µs |
| small/config | 307.826 ± 3.577 ns | 1.574 ± 0.021 µs | 266.704 ± 2.591 ns | **241.195 ± 5.965 ns** | 10.121 ± 0.061 µs | 579.483 ± 7.645 ns | 1.753 ± 0.006 µs |
| small/prettierrc | **566.350 ± 3.695 ns** | 2.614 ± 0.023 µs | 963.998 ± 8.824 ns | 856.029 ± 9.877 ns | 5.507 ± 0.048 µs | 1.632 ± 0.017 µs | 2.095 ± 0.009 µs |

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
| *startup* | 275.968 ± 13.776 ms | 174.066 ± 2.701 ms | 2.587 ± 0.190 ms | 439.078 ± 224.615 ms † | 103.969 ± 1.545 ms |
| generated/deep-nesting | 291.243 ± 6.540 ms | 194.840 ± 1.528 ms | **7.180 ± 0.452 ms** | 331.521 ± 4.662 ms | 128.726 ± 2.389 ms |
| generated/large-array | 278.572 ± 6.081 ms | 182.845 ± 2.311 ms | **3.447 ± 0.338 ms** | 316.475 ± 2.429 ms | 106.923 ± 2.986 ms |
| generated/many-defs | 533.221 ± 12.380 ms | 264.526 ± 1.741 ms | **32.548 ± 0.645 ms** | 490.126 ± 3.866 ms | 478.398 ± 4.187 ms |
| generated/wide-object | 703.018 ± 9.089 ms | 315.919 ± 2.892 ms | **27.741 ± 0.464 ms** | 523.970 ± 4.509 ms | 552.038 ± 2.968 ms |
| large/api-catalog | 284.052 ± 5.272 ms | 191.524 ± 3.055 ms | **3.329 ± 0.039 ms** | 323.274 ± 2.455 ms | 111.088 ± 1.629 ms |
| large/circleci | 371.216 ± 4.216 ms | 224.865 ± 3.535 ms | **13.540 ± 0.591 ms** | — | — |
| massive/data-warehouse | 1.234 ± 0.019 s | 358.579 ± 3.647 ms | **49.816 ± 1.165 ms** | 673.100 ± 4.712 ms | 860.323 ± 4.966 ms |
| massive/k8s-deployment | 659.328 ± 249.489 ms † | 290.263 ± 10.971 ms | **33.139 ± 10.057 ms †** | 462.937 ± 28.967 ms | 151.001 ± 17.456 ms † |
| medium/package-json | — | — | 108.055 ± 1.469 ms | — | — |
| medium/user-profile | 222.771 ± 2.763 ms | 161.193 ± 3.889 ms | **2.345 ± 0.240 ms †** | 279.231 ± 2.324 ms | 90.844 ± 2.182 ms |
| small/config | 212.983 ± 0.840 ms | 156.171 ± 1.288 ms | **1.964 ± 0.138 ms** | 270.158 ± 1.572 ms | 86.699 ± 1.601 ms |
| small/prettierrc | 255.540 ± 0.869 ms | 169.453 ± 1.896 ms | **3.203 ± 0.034 ms** | 292.135 ± 2.391 ms | — |

### codegen — peak RSS

| Fixture | datamodel-code-generator | json-schema-to-typescript | jsonschemars | quicktype | typify |
|---|---|---|---|---|---|
| *startup* | 59.64 ± 0.08 MiB | 74.56 ± 0.39 MiB | 3.70 MiB | 127.39 ± 0.75 MiB | 31.69 ± 0.01 MiB |
| generated/deep-nesting | 61.81 MiB | 79.56 ± 0.26 MiB | **7.64 MiB** | 168.47 MiB | 38.50 ± 0.05 MiB |
| generated/large-array | 60.66 ± 0.09 MiB | 76.55 ± 0.19 MiB | **4.66 ± 0.01 MiB** | 131.73 ± 0.17 MiB | 33.19 ± 0.01 MiB |
| generated/many-defs | 83.20 ± 0.08 MiB | 123.52 MiB | **22.45 ± 0.27 MiB** | 205.77 ± 0.06 MiB | 98.02 ± 0.31 MiB |
| generated/wide-object | 97.62 ± 0.16 MiB | 169.31 MiB | **23.88 MiB** | 223.33 ± 0.27 MiB | 116.00 MiB |
| large/api-catalog | 63.39 ± 0.21 MiB | 76.70 ± 0.18 MiB | **5.31 ± 0.03 MiB** | 143.39 ± 0.36 MiB | 34.55 ± 0.05 MiB |
| large/circleci | 70.61 ± 0.08 MiB | 92.70 ± 0.47 MiB | **11.25 ± 0.03 MiB** | — | — |
| massive/data-warehouse | 176.55 ± 0.33 MiB | 178.28 MiB | **31.22 ± 0.03 MiB** | 226.81 MiB | 176.52 ± 0.21 MiB |
| massive/k8s-deployment | 88.72 ± 0.27 MiB | 131.59 ± 0.24 MiB | **17.98 ± 0.07 MiB** | 214.84 ± 6.14 MiB | 31.69 ± 0.01 MiB |
| medium/package-json | — | — | 43.81 ± 0.57 MiB | — | — |
| medium/user-profile | 60.19 ± 0.03 MiB | 75.39 ± 0.17 MiB | **4.86 ± 0.02 MiB** | 133.14 ± 0.17 MiB | 33.44 MiB |
| small/config | 58.66 ± 0.02 MiB | 75.70 MiB | **4.41 MiB** | 127.88 MiB | 32.78 MiB |
| small/prettierrc | 66.33 ± 0.07 MiB | 76.83 MiB | **5.58 ± 0.01 MiB** | 148.22 ± 0.04 MiB | — |

### validate (valid) — time

| Fixture | ajv | blaze | boon | fastjsonschema | jsonschema-cli | jsonschemars | python-jsonschema |
|---|---|---|---|---|---|---|---|
| *startup* | 127.859 ± 2.148 ms | 5.544 ± 2.723 ms † | 3.734 ± 0.364 ms | 33.857 ± 3.339 ms | 4.821 ± 0.252 ms | 2.419 ± 0.192 ms | 76.954 ± 2.946 ms |
| generated/deep-nesting | 154.110 ± 1.627 ms | 17.331 ± 0.643 ms | 5.332 ± 0.620 ms † | 114.311 ± 1.694 ms | 5.608 ± 0.198 ms | **2.729 ± 0.235 ms** | 93.937 ± 2.999 ms |
| generated/large-array | 174.789 ± 2.370 ms | 110.952 ± 1.600 ms | 64.629 ± 1.009 ms | 167.046 ± 1.994 ms | 50.488 ± 0.535 ms | 49.526 ± 0.707 ms | 1.479 ± 0.010 s |
| generated/many-defs | 267.054 ± 2.507 ms | 127.888 ± 0.905 ms | 25.930 ± 1.140 ms | 425.730 ± 3.567 ms | 13.540 ± 0.289 ms | **8.672 ± 0.541 ms** | 448.441 ± 4.119 ms |
| generated/wide-object | — | 65.586 ± 1.414 ms | 32.005 ± 1.477 ms | 345.640 ± 4.489 ms | 10.027 ± 0.359 ms | **4.668 ± 0.166 ms** | 325.032 ± 4.011 ms |
| large/api-catalog | 144.385 ± 1.861 ms | 6.602 ± 1.188 ms † | 4.259 ± 0.306 ms | 48.584 ± 2.141 ms | 5.114 ± 0.198 ms | **2.722 ± 0.366 ms †** | 85.570 ± 1.801 ms |
| large/circleci | 186.595 ± 3.644 ms | 32.454 ± 0.672 ms | 7.844 ± 0.568 ms | 367.589 ± 3.548 ms | 7.253 ± 0.184 ms | **3.552 ± 0.063 ms** | 88.503 ± 3.710 ms |
| massive/data-warehouse | 628.320 ± 4.492 ms | 278.364 ± 3.599 ms | 57.010 ± 2.008 ms | 1.323 ± 0.009 s | 14.403 ± 0.371 ms | **6.741 ± 0.672 ms** | 499.261 ± 2.954 ms |
| massive/k8s-deployment | — | — | 13.515 ± 0.385 ms | 403.825 ± 5.871 ms | 270.229 ± 33.365 ms † | **3.678 ± 0.357 ms** | 242.216 ± 3.322 ms |
| medium/package-json | 928.573 ± 32.771 ms | 403.759 ± 5.986 ms | 83.068 ± 1.526 ms | 983.613 ± 31.027 ms | — | **8.233 ± 0.695 ms** | 92.392 ± 1.593 ms |
| medium/user-profile | 123.222 ± 1.283 ms | 3.802 ± 0.142 ms | 2.961 ± 0.125 ms | 28.627 ± 0.849 ms | 3.821 ± 0.096 ms | **1.727 ± 0.037 ms** | 61.872 ± 0.536 ms |
| small/config | 120.158 ± 1.194 ms | 3.278 ± 0.158 ms | 2.839 ± 0.225 ms | 25.922 ± 0.577 ms | 3.804 ± 0.446 ms † | **1.619 ± 0.037 ms** | 60.335 ± 0.240 ms |
| small/prettierrc | 124.208 ± 1.061 ms | 5.808 ± 0.142 ms | 3.102 ± 0.217 ms | 38.219 ± 0.431 ms | 3.789 ± 0.465 ms † | **1.744 ± 0.033 ms** | 61.414 ± 0.280 ms |

### validate (valid) — peak RSS

| Fixture | ajv | blaze | boon | fastjsonschema | jsonschema-cli | jsonschemars | python-jsonschema |
|---|---|---|---|---|---|---|---|
| *startup* | 53.67 MiB | 8.52 ± 0.01 MiB | 5.73 MiB | 17.28 MiB | 8.44 ± 0.00 MiB | 3.47 MiB | 25.16 ± 0.10 MiB |
| generated/deep-nesting | 62.83 ± 0.07 MiB | 18.09 MiB | 7.00 ± 0.02 MiB | 89.45 MiB | 10.03 ± 0.00 MiB | **4.16 ± 0.01 MiB** | 25.33 MiB |
| generated/large-array | 85.02 ± 0.22 MiB | 300.80 MiB | **55.94 MiB** | 60.42 ± 0.09 MiB | 64.91 ± 0.02 MiB | 59.72 MiB | 65.94 ± 0.06 MiB |
| generated/many-defs | 81.47 ± 0.42 MiB | 84.83 MiB | 20.86 MiB | 329.58 MiB | 21.67 ± 0.15 MiB | **13.48 ± 0.17 MiB** | 28.58 ± 0.08 MiB |
| generated/wide-object | — | 26.86 MiB | 16.89 ± 0.00 MiB | 283.92 ± 1.07 MiB | 15.48 ± 0.01 MiB | **7.34 ± 0.10 MiB** | 26.39 ± 0.01 MiB |
| large/api-catalog | 55.88 ± 0.11 MiB | 9.55 MiB | 6.00 ± 0.06 MiB | 27.58 ± 0.14 MiB | 10.56 ± 0.01 MiB | **3.95 ± 0.01 MiB** | 25.14 ± 0.05 MiB |
| large/circleci | 75.98 ± 0.13 MiB | 19.53 MiB | 9.33 ± 0.05 MiB | 271.66 ± 0.13 MiB | 13.25 ± 0.03 MiB | **5.48 MiB** | 25.50 MiB |
| massive/data-warehouse | 152.69 ± 2.15 MiB | 297.67 MiB | 22.80 ± 0.10 MiB | 974.52 ± 1.50 MiB | 21.41 MiB | **9.52 MiB** | 27.42 ± 0.13 MiB |
| massive/k8s-deployment | — | — | 13.31 MiB | 318.62 ± 0.49 MiB | 20.53 ± 0.13 MiB | **6.02 MiB** | 26.42 ± 0.05 MiB |
| medium/package-json | 363.20 ± 0.27 MiB | 68.64 MiB | 28.98 ± 0.17 MiB | 661.91 ± 0.76 MiB | — | **11.50 MiB** | 28.33 MiB |
| medium/user-profile | 54.70 MiB | 9.22 MiB | 5.88 ± 0.03 MiB | 20.69 ± 0.11 MiB | 10.36 ± 0.01 MiB | **3.83 ± 0.01 MiB** | 24.77 ± 0.03 MiB |
| small/config | 54.28 ± 0.03 MiB | 8.91 MiB | 5.81 ± 0.03 MiB | 18.05 ± 0.02 MiB | 10.16 ± 0.01 MiB | **3.72 MiB** | 24.64 ± 0.02 MiB |
| small/prettierrc | 57.92 MiB | 10.09 ± 0.01 MiB | 5.94 ± 0.01 MiB | 30.06 MiB | 9.22 MiB | **3.89 MiB** | 24.64 ± 0.01 MiB |

### validate (invalid) — time

| Fixture | ajv | blaze | boon | fastjsonschema | jsonschema-cli | jsonschemars | python-jsonschema |
|---|---|---|---|---|---|---|---|
| *startup* | 140.676 ± 12.217 ms | 4.936 ± 0.186 ms | 3.362 ± 0.256 ms | 34.221 ± 6.124 ms † | 4.360 ± 0.514 ms † | 2.444 ± 0.290 ms † | 76.063 ± 8.752 ms † |
| generated/deep-nesting | 155.680 ± 1.978 ms | 39.887 ± 0.959 ms | 5.303 ± 0.717 ms † | 113.522 ± 1.907 ms | 5.611 ± 0.258 ms | **2.771 ± 0.243 ms** | 96.095 ± 2.037 ms |
| generated/large-array | 177.224 ± 2.225 ms | 129.284 ± 0.875 ms | 65.838 ± 1.197 ms | 165.855 ± 2.952 ms | 50.505 ± 7.158 ms † | 49.866 ± 1.303 ms | 1.507 ± 0.011 s |
| generated/many-defs | 267.107 ± 1.162 ms | 132.789 ± 0.818 ms | 27.003 ± 1.171 ms | 426.036 ± 4.348 ms | 13.314 ± 0.509 ms | **8.446 ± 0.442 ms** | 453.698 ± 3.787 ms |
| generated/wide-object | — | 66.304 ± 0.810 ms | 31.798 ± 1.365 ms | 341.465 ± 5.031 ms | 9.886 ± 0.350 ms | **5.021 ± 0.393 ms** | 323.533 ± 2.461 ms |
| large/api-catalog | 144.259 ± 1.481 ms | 7.164 ± 0.515 ms | 4.449 ± 0.583 ms † | 47.234 ± 2.462 ms | 5.468 ± 0.509 ms | **2.660 ± 0.105 ms** | 85.101 ± 2.128 ms |
| large/circleci | 186.331 ± 1.804 ms | 33.412 ± 0.938 ms | 8.309 ± 0.454 ms | 368.664 ± 4.728 ms | 6.914 ± 0.181 ms | **3.672 ± 0.353 ms** | 90.169 ± 2.454 ms |
| massive/data-warehouse | 557.335 ± 6.542 ms | 344.772 ± 3.030 ms | 58.522 ± 1.980 ms | 1.330 ± 0.506 s † | 14.273 ± 0.463 ms | **6.726 ± 0.786 ms †** | 503.211 ± 5.420 ms |
| massive/k8s-deployment | — | — | 13.437 ± 0.329 ms | 403.999 ± 6.820 ms | 262.509 ± 34.099 ms † | **3.620 ± 0.263 ms** | 242.352 ± 3.315 ms |
| medium/package-json | 963.353 ± 36.274 ms | 410.536 ± 6.797 ms | 82.791 ± 0.854 ms | 883.662 ± 18.899 ms | — | **8.809 ± 0.461 ms** | 89.665 ± 1.693 ms |
| medium/user-profile | 122.702 ± 2.792 ms | 4.058 ± 0.330 ms | 3.011 ± 0.147 ms | 28.489 ± 0.330 ms | 3.845 ± 0.130 ms | **1.760 ± 0.077 ms** | 62.139 ± 0.506 ms |
| small/config | 120.950 ± 1.002 ms | 3.375 ± 0.066 ms | 2.986 ± 0.236 ms | 25.733 ± 0.290 ms | 3.750 ± 0.047 ms | **1.683 ± 0.084 ms** | 60.436 ± 0.728 ms |
| small/prettierrc | 123.502 ± 0.755 ms | 6.304 ± 0.381 ms | 3.167 ± 0.100 ms | 38.158 ± 0.298 ms | 3.802 ± 0.065 ms | **1.751 ± 0.062 ms** | 61.089 ± 0.849 ms |

### validate (invalid) — peak RSS

| Fixture | ajv | blaze | boon | fastjsonschema | jsonschema-cli | jsonschemars | python-jsonschema |
|---|---|---|---|---|---|---|---|
| *startup* | 54.61 ± 0.07 MiB | 8.67 MiB | 5.67 ± 0.01 MiB | 17.34 ± 0.01 MiB | 8.58 MiB | 3.53 MiB | 25.27 MiB |
| generated/deep-nesting | 63.05 MiB | 18.27 ± 0.01 MiB | 7.03 ± 0.01 MiB | 90.53 ± 0.69 MiB | 10.16 MiB | **4.25 ± 0.01 MiB** | 25.55 ± 0.17 MiB |
| generated/large-array | 84.27 ± 0.40 MiB | 326.53 MiB | **55.95 ± 0.01 MiB** | 60.55 ± 0.15 MiB | 65.03 MiB | 59.75 ± 0.01 MiB | 65.67 MiB |
| generated/many-defs | 81.06 ± 0.36 MiB | 85.44 MiB | 20.89 ± 0.01 MiB | 328.80 ± 1.13 MiB | 21.69 MiB | **13.25 ± 0.02 MiB** | 28.52 MiB |
| generated/wide-object | — | 27.56 MiB | 16.66 MiB | 283.08 ± 0.50 MiB | 15.67 ± 0.03 MiB | **7.38 ± 0.01 MiB** | 26.77 MiB |
| large/api-catalog | 55.84 ± 0.09 MiB | 9.69 MiB | 6.02 ± 0.01 MiB | 27.77 ± 0.12 MiB | 10.70 MiB | **4.00 ± 0.01 MiB** | 24.73 ± 0.28 MiB |
| large/circleci | 75.73 MiB | 19.48 ± 0.01 MiB | 9.22 ± 0.02 MiB | 271.44 ± 0.38 MiB | 13.28 ± 0.01 MiB | **5.55 ± 0.00 MiB** | 25.72 ± 0.05 MiB |
| massive/data-warehouse | 134.98 ± 1.22 MiB | 68.66 MiB | 24.14 ± 0.18 MiB | 971.91 ± 0.50 MiB | 21.89 MiB | **9.70 ± 0.00 MiB** | 28.33 ± 0.09 MiB |
| massive/k8s-deployment | — | — | 13.28 ± 0.14 MiB | 318.64 MiB | 20.52 MiB | **6.02 MiB** | 26.36 ± 0.13 MiB |
| medium/package-json | 362.61 ± 0.22 MiB | 68.72 MiB | 28.98 ± 0.02 MiB | 663.22 ± 0.47 MiB | — | **11.52 ± 0.15 MiB** | 28.52 MiB |
| medium/user-profile | 54.73 MiB | 9.31 MiB | 5.88 ± 0.02 MiB | 20.48 MiB | 10.50 MiB | **3.86 MiB** | 24.64 ± 0.01 MiB |
| small/config | 54.41 ± 0.02 MiB | 9.06 MiB | 5.86 MiB | 18.02 ± 0.05 MiB | 10.30 ± 0.02 MiB | **3.81 ± 0.01 MiB** | 24.70 MiB |
| small/prettierrc | 57.94 ± 0.36 MiB | 10.19 ± 0.01 MiB | 5.94 ± 0.01 MiB | 30.08 MiB | 9.36 ± 0.01 MiB | **3.98 MiB** | 24.72 ± 0.02 MiB |

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
