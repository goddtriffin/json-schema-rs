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
| in-process | codegen | peak heap | jsonschemars | 7 / 9 |
| in-process | schema compile | time | jsonschemars | 11 / 11 |
| in-process | schema compile | peak heap | jsonschemars | 11 / 11 |
| in-process | validate (valid) | time | jsonschema | 11 / 11 |
| in-process | validate (valid) | peak heap | jsonschema | 11 / 11 |
| in-process | validate (invalid) | time | jsonschema | 10 / 11 |
| in-process | validate (invalid) | peak heap | jsonschema | 9 / 11 |
| CLI | codegen | time | jsonschemars | 9 / 9 |
| CLI | codegen | peak RSS | jsonschemars | 8 / 9 |
| CLI | validate (valid) | time | jsonschemars | 9 / 11 |
| CLI | validate (valid) | peak RSS | jsonschemars | 10 / 11 |
| CLI | validate (invalid) | time | jsonschemars | 9 / 11 |
| CLI | validate (invalid) | peak RSS | jsonschemars | 10 / 11 |

## In-process (library calls)

### codegen — time

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | **4.456 ± 0.007 ms** | 9.680 ± 0.047 ms |
| generated/large-array | **62.469 ± 0.055 µs** | 1.024 ± 0.002 ms |
| generated/many-defs | **26.943 ± 0.045 ms** | 122.613 ± 2.232 ms |
| generated/wide-object | **135.360 ± 0.234 ms** | 147.653 ± 0.221 ms |
| large/api-catalog | **195.103 ± 1.680 µs** | 3.091 ± 0.008 ms |
| large/circleci | 1.617 ± 0.003 ms | — |
| massive/data-warehouse | **7.774 ± 0.078 ms** | 269.015 ± 3.861 ms |
| massive/k8s-deployment | 14.870 ± 0.124 ms | **1.843 ± 0.006 ms** |
| medium/user-profile | **80.951 ± 0.383 µs** | 1.416 ± 0.003 ms |
| small/config | **26.673 ± 0.030 µs** | 509.101 ± 4.687 µs |

### codegen — peak heap

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | 19.03 MiB | **12.65 MiB** |
| generated/large-array | **250.76 KiB** | 1.23 MiB |
| generated/many-defs | **41.42 MiB** | 133.38 MiB |
| generated/wide-object | **19.36 MiB** | 162.36 MiB |
| large/api-catalog | **667.22 KiB** | 3.64 MiB |
| large/circleci | 5.90 MiB | — |
| massive/data-warehouse | **24.57 MiB** | 288.90 MiB |
| massive/k8s-deployment | 47.59 MiB | **2.79 MiB** |
| medium/user-profile | **248.33 KiB** | 1.71 MiB |
| small/config | **95.91 KiB** | 639.58 KiB |

### schema compile — time

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 1.026 ± 0.015 ms | 205.539 ± 0.901 µs | **69.754 ± 0.072 µs** |
| generated/large-array | 44.616 ± 0.102 µs | 20.634 ± 0.037 µs | **6.101 ± 0.014 µs** |
| generated/many-defs | 13.632 ± 0.022 ms | 2.998 ± 0.004 ms | **1.031 ± 0.013 ms** |
| generated/wide-object | 23.239 ± 0.074 ms | 2.494 ± 0.004 ms | **806.193 ± 0.727 µs** |
| large/api-catalog | 177.784 ± 0.226 µs | 103.576 ± 0.244 µs | **23.958 ± 0.090 µs** |
| large/circleci | 2.867 ± 0.013 ms | 1.020 ± 0.006 ms | **300.042 ± 0.387 µs** |
| massive/data-warehouse | 44.314 ± 0.229 ms | 6.261 ± 0.009 ms | **1.550 ± 0.009 ms** |
| massive/k8s-deployment | 7.313 ± 0.021 ms | 1.596 ± 0.008 ms | **493.090 ± 0.415 µs** |
| medium/user-profile | 66.629 ± 0.127 µs | 40.100 ± 0.112 µs | **9.075 ± 0.012 µs** |
| small/config | 26.730 ± 0.050 µs | 18.709 ± 0.054 µs | **3.206 ± 0.014 µs** |
| small/prettierrc | 203.661 ± 0.293 µs | 111.941 ± 0.081 µs | **32.635 ± 0.087 µs** |

### schema compile — peak heap

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 636.04 KiB | 534.41 KiB | **118.03 KiB** |
| generated/large-array | 66.67 KiB | 34.10 KiB | **17.26 KiB** |
| generated/many-defs | 8.10 MiB | 3.88 MiB | **2.30 MiB** |
| generated/wide-object | 13.73 MiB | 4.03 MiB | **2.18 MiB** |
| large/api-catalog | 266.71 KiB | 151.24 KiB | **52.19 KiB** |
| large/circleci | 3.85 MiB | 1.75 MiB | **737.98 KiB** |
| massive/data-warehouse | 16.73 MiB | 9.42 MiB | **3.37 MiB** |
| massive/k8s-deployment | 7.45 MiB | 3.28 MiB | **1.22 MiB** |
| medium/user-profile | 121.82 KiB | 59.13 KiB | **22.40 KiB** |
| small/config | 38.35 KiB | 24.08 KiB | **8.30 KiB** |
| small/prettierrc | 476.85 KiB | 261.04 KiB | **97.81 KiB** |

### validate (valid) — time

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.405 ± 0.013 µs | **2.398 ± 0.025 µs** | 140.826 ± 1.524 µs |
| generated/large-array | 17.878 ± 0.072 ms | **7.491 ± 0.043 ms** | 146.704 ± 1.734 ms |
| generated/many-defs | 1.726 ± 0.006 ms | **619.571 ± 1.532 µs** | 18.845 ± 0.219 ms |
| generated/wide-object | 172.101 ± 0.304 µs | **110.883 ± 0.327 µs** | 666.421 ± 1.451 µs |
| large/api-catalog | 564.362 ± 3.250 ns | **242.541 ± 1.676 ns** | 6.734 ± 0.014 µs |
| large/circleci | 2.197 ± 0.061 µs | **463.170 ± 0.850 ns** | 2.095 ± 0.005 µs |
| massive/data-warehouse | 69.678 ± 0.284 µs | **23.691 ± 0.156 µs** | 620.396 ± 0.695 µs |
| massive/k8s-deployment | 517.794 ± 1.063 ns | **207.824 ± 0.590 ns** | 4.695 ± 0.012 µs |
| medium/user-profile | 833.739 ± 2.098 ns | **372.431 ± 0.891 ns** | 5.947 ± 0.010 µs |
| small/config | 224.859 ± 0.798 ns | **113.229 ± 0.184 ns** | 2.823 ± 0.006 µs |
| small/prettierrc | 719.870 ± 3.202 ns | **171.698 ± 1.008 ns** | 2.962 ± 0.009 µs |

### validate (valid) — peak heap

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 2.25 KiB | **0.03 KiB** | 11.86 KiB |
| generated/large-array | 0.19 KiB | **0.03 KiB** | 5.97 MiB |
| generated/many-defs | 0.56 KiB | **0.03 KiB** | 18.42 KiB |
| generated/wide-object | 0.19 KiB | **0.03 KiB** | 697.51 KiB |
| large/api-catalog | 0.19 KiB | **0.03 KiB** | 3.01 KiB |
| large/circleci | 2.58 KiB | **0.03 KiB** | 1.37 KiB |
| massive/data-warehouse | 0.19 KiB | **0.03 KiB** | 13.77 KiB |
| massive/k8s-deployment | 0.19 KiB | **0.03 KiB** | 1.68 KiB |
| medium/user-profile | 0.19 KiB | **0.03 KiB** | 3.82 KiB |
| small/config | 0.19 KiB | **0.03 KiB** | 3.09 KiB |
| small/prettierrc | 0.77 KiB | **0.03 KiB** | 3.68 KiB |

### validate (invalid) — time

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 7.547 ± 0.017 µs | **2.779 ± 0.005 µs** | 140.857 ± 0.135 µs |
| generated/large-array | 17.915 ± 0.071 ms | **7.452 ± 0.085 ms** | 149.243 ± 0.301 ms |
| generated/many-defs | 1.722 ± 0.006 ms | **619.680 ± 21.655 µs** | 18.795 ± 0.097 ms |
| generated/wide-object | 171.654 ± 0.438 µs | **110.995 ± 0.505 µs** | 669.118 ± 6.399 µs |
| large/api-catalog | 657.102 ± 0.987 ns | **392.881 ± 2.273 ns** | 5.440 ± 0.016 µs |
| large/circleci | 135.961 ± 0.218 ns | **131.650 ± 0.596 ns** | 418.259 ± 1.880 ns |
| massive/data-warehouse | 180.061 ± 0.323 µs | **137.200 ± 0.389 µs** | 910.379 ± 3.904 µs |
| massive/k8s-deployment | 182.261 ± 0.409 ns | **132.675 ± 0.265 ns** | 582.236 ± 3.069 ns |
| medium/user-profile | 369.958 ± 4.798 ns | **317.860 ± 1.560 ns** | 2.434 ± 0.013 µs |
| small/config | 290.827 ± 0.319 ns | **244.335 ± 4.582 ns** | 2.875 ± 0.010 µs |
| small/prettierrc | **523.673 ± 0.749 ns** | 897.928 ± 1.444 ns | 912.501 ± 3.917 ns |

### validate (invalid) — peak heap

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 2.97 KiB | **0.75 KiB** | 11.86 KiB |
| generated/large-array | 1.37 KiB | **0.46 KiB** | 5.97 MiB |
| generated/many-defs | 7.77 KiB | **0.77 KiB** | 18.42 KiB |
| generated/wide-object | 0.74 KiB | **0.41 KiB** | 697.51 KiB |
| large/api-catalog | 1.44 KiB | **0.83 KiB** | 3.18 KiB |
| large/circleci | 0.74 KiB | **0.40 KiB** | 1.29 KiB |
| massive/data-warehouse | 467.89 KiB | 111.78 KiB | **4.72 KiB** |
| massive/k8s-deployment | 0.81 KiB | **0.48 KiB** | 1.07 KiB |
| medium/user-profile | 1.30 KiB | **0.73 KiB** | 1.91 KiB |
| small/config | 1.30 KiB | **0.71 KiB** | 4.12 KiB |
| small/prettierrc | 2.38 KiB | 2.69 KiB | **1.73 KiB** |

## CLI (end to end)

### codegen — time

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | **8.402 ± 0.385 ms** | 107.563 ± 0.631 ms |
| generated/large-array | **1.819 ± 0.042 ms** | 86.816 ± 0.369 ms |
| generated/many-defs | **32.501 ± 0.210 ms** | 435.321 ± 1.098 ms |
| generated/wide-object | **138.491 ± 1.829 ms** | 504.431 ± 1.066 ms |
| large/api-catalog | **2.138 ± 0.046 ms** | 93.329 ± 0.579 ms |
| large/circleci | 4.198 ± 0.055 ms | — |
| massive/data-warehouse | **12.233 ± 0.137 ms** | 783.488 ± 2.864 ms |
| massive/k8s-deployment | **21.984 ± 0.157 ms** | 85.916 ± 0.199 ms |
| medium/user-profile | **1.862 ± 0.024 ms** | 88.066 ± 1.008 ms |
| small/config | **1.735 ± 0.049 ms** | 84.896 ± 0.942 ms |

### codegen — peak RSS

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | **24.19 MiB** | 38.02 ± 0.01 MiB |
| generated/large-array | **3.81 ± 0.00 MiB** | 33.30 ± 0.00 MiB |
| generated/many-defs | **57.02 ± 0.00 MiB** | 98.05 MiB |
| generated/wide-object | **24.95 ± 0.01 MiB** | 113.91 ± 0.48 MiB |
| large/api-catalog | **4.70 ± 0.02 MiB** | 34.77 ± 0.06 MiB |
| large/circleci | 10.73 ± 0.01 MiB | — |
| massive/data-warehouse | **32.80 MiB** | 175.92 MiB |
| massive/k8s-deployment | 57.52 ± 0.01 MiB | **31.80 MiB** |
| medium/user-profile | **3.86 ± 0.02 MiB** | 33.53 ± 0.03 MiB |
| small/config | **3.58 ± 0.01 MiB** | 32.94 MiB |

### validate (valid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.423 ± 0.498 ms † | 3.985 ± 0.063 ms | **1.901 ± 0.036 ms** |
| generated/large-array | 58.638 ± 0.301 ms | **44.739 ± 0.460 ms** | 187.174 ± 1.594 ms |
| generated/many-defs | 22.908 ± 0.230 ms | **11.341 ± 0.123 ms** | 25.971 ± 0.264 ms |
| generated/wide-object | 27.863 ± 0.297 ms | 8.041 ± 0.151 ms | **3.906 ± 0.030 ms** |
| large/api-catalog | 3.045 ± 0.038 ms | 3.806 ± 0.061 ms | **1.611 ± 0.033 ms** |
| large/circleci | 6.738 ± 0.052 ms | 5.533 ± 0.067 ms | **2.066 ± 0.038 ms** |
| massive/data-warehouse | 49.723 ± 0.345 ms | 11.761 ± 0.097 ms | **4.681 ± 0.077 ms** |
| massive/k8s-deployment | 11.761 ± 0.090 ms | 261.868 ± 50.104 ms † | **2.426 ± 0.044 ms** |
| medium/user-profile | 2.907 ± 0.057 ms | 3.676 ± 0.070 ms | **1.609 ± 0.047 ms** |
| small/config | 2.834 ± 0.052 ms | 3.592 ± 0.078 ms | **1.549 ± 0.038 ms** |
| small/prettierrc | 3.063 ± 0.032 ms | 3.667 ± 0.194 ms | **1.617 ± 0.034 ms** |

### validate (valid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.81 ± 0.01 MiB | 9.89 MiB | **3.91 ± 0.04 MiB** |
| generated/large-array | **55.78 ± 0.01 MiB** | 64.70 ± 0.02 MiB | 67.34 MiB |
| generated/many-defs | 19.09 ± 0.00 MiB | 21.38 ± 0.02 MiB | **12.48 MiB** |
| generated/wide-object | 15.09 ± 0.01 MiB | 15.28 MiB | **6.67 MiB** |
| large/api-catalog | 5.86 MiB | 10.33 ± 0.01 MiB | **3.42 ± 0.01 MiB** |
| large/circleci | 9.09 ± 0.00 MiB | 13.06 ± 0.04 MiB | **4.52 MiB** |
| massive/data-warehouse | 21.95 ± 0.01 MiB | 20.94 MiB | **8.38 ± 0.01 MiB** |
| massive/k8s-deployment | 11.86 ± 0.02 MiB | 20.56 ± 0.05 MiB | **5.16 MiB** |
| medium/user-profile | 5.75 ± 0.01 MiB | 10.06 MiB | **3.33 MiB** |
| small/config | 5.77 MiB | 9.86 MiB | **3.27 ± 0.01 MiB** |
| small/prettierrc | 5.83 ± 0.02 MiB | 8.94 ± 0.01 MiB | **3.36 MiB** |

### validate (invalid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.047 ± 0.038 ms | 4.114 ± 0.074 ms | **1.913 ± 0.039 ms** |
| generated/large-array | 58.646 ± 0.587 ms | **44.698 ± 0.530 ms** | 187.190 ± 1.558 ms |
| generated/many-defs | 23.049 ± 0.254 ms | **11.386 ± 0.136 ms** | 26.011 ± 0.284 ms |
| generated/wide-object | 27.941 ± 0.684 ms | 7.904 ± 0.052 ms | **3.906 ± 0.035 ms** |
| large/api-catalog | 3.065 ± 0.032 ms | 3.819 ± 0.061 ms | **1.593 ± 0.023 ms** |
| large/circleci | 6.743 ± 0.050 ms | 5.614 ± 0.104 ms | **2.069 ± 0.036 ms** |
| massive/data-warehouse | 51.111 ± 0.505 ms | 11.926 ± 0.140 ms | **4.860 ± 0.049 ms** |
| massive/k8s-deployment | 11.792 ± 0.095 ms | 247.985 ± 43.670 ms † | **2.449 ± 0.043 ms** |
| medium/user-profile | 2.913 ± 0.065 ms | 3.679 ± 0.080 ms | **1.552 ± 0.029 ms** |
| small/config | 2.830 ± 0.032 ms | 3.601 ± 0.073 ms | **1.552 ± 0.029 ms** |
| small/prettierrc | 3.111 ± 0.050 ms | 3.674 ± 0.095 ms | **1.584 ± 0.040 ms** |

### validate (invalid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.80 ± 0.02 MiB | 10.02 MiB | **4.03 MiB** |
| generated/large-array | **55.80 ± 0.01 MiB** | 64.83 ± 0.02 MiB | 67.33 ± 0.04 MiB |
| generated/many-defs | 19.09 ± 0.01 MiB | 21.42 MiB | **12.53 MiB** |
| generated/wide-object | 15.66 ± 0.26 MiB | 15.41 MiB | **6.72 MiB** |
| large/api-catalog | 5.88 MiB | 10.50 ± 0.01 MiB | **3.41 ± 0.01 MiB** |
| large/circleci | 9.14 ± 0.02 MiB | 13.05 ± 0.01 MiB | **4.52 ± 0.01 MiB** |
| massive/data-warehouse | 22.64 MiB | 21.06 ± 0.02 MiB | **8.16 MiB** |
| massive/k8s-deployment | 11.89 MiB | 20.66 ± 0.07 MiB | **5.19 MiB** |
| medium/user-profile | 5.75 ± 0.01 MiB | 10.19 ± 0.03 MiB | **3.33 MiB** |
| small/config | 5.75 ± 0.02 MiB | 10.02 MiB | **3.31 MiB** |
| small/prettierrc | 5.81 MiB | 9.11 ± 0.01 MiB | **3.34 ± 0.01 MiB** |

## Compatibility

203 of 228 attempted cells pass. Rows with at least one failure:

| Tier | Operation | Fixture | boon | jsonschema | jsonschema-cli | jsonschemars | typify |
|---|---|---|---|---|---|---|---|
| in-process | codegen | large/circleci | — | — | — | pass | FAIL: panic: not implemented: if/then/else schemas are not supported: { "if": { "required": [ "type" ], "properties": { "type": { "const": "release" } } }, "then": { "required": [ "plan_name" ], "properties": { "plan_name": { "type": "string", "markdownDescription": "Required plan name for release jobs." } }, "additionalProperties": true, "markdownDescription": "https://circleci.com/docs/reference/configuration-reference/#job-type\n\nRelease job." }, "else": { "if": { "required": [ "type" ], "properties": { "type": { "enum": [ "lock", "unlock" ] } } }, "then": { "required": [ "key" ], "properties": { "key": { "type": "string", "markdownDescription": "Required key for lock/unlock jobs." } }, "additionalProperties": true, "markdownDescription": "https://circleci.com/docs/reference/configuration-reference/#job-type\n\nA lock/unlock job" }, "else": { "if": { "required": [ "type" ], "properties": { "type": { "enum": [ "approval", "no-op" ] } } }, "then": { "additionalProperties": true, "markdownDescription": "https://circleci.com/docs/reference/configuration-reference/#job-type\n\nApproval/no-op jobs primarily act as workflow markers. Other keys like `steps:` are permitted but will be ignored in the final pipeline." }, "else": { "if": { "anyOf": [ { "not": { "required": [ "type" ] } }, { "required": [ "type" ], "properties": { "type": { "const": "build" } } } ], "markdownDescription": "https://circleci.com/docs/reference/configuration-reference/#job-type\n\nA build job. The job explicitly has type 'build' or defaults to build when no type specified." }, "then": { "anyOf": [ { "type": "object", "required": [ "executor" ], "markdownDescription": "A job that uses an executor defined elsewhere in the configuration." }, { "oneOf": [ { "type": "object", "required": [ "machine" ] }, { "type": "object", "required": [ "docker" ] }, { "type": "object", "required": [ "macos" ] } ] } ], "required": [ "steps" ], "properties": { "circleci_ip_ranges": { "type": "boolean", "markdownDescription": "Enable running jobs with a set of well-defined IP address ranges." }, "description": { "type": "string" }, "docker": { "type": "array", "items": { "type": "object", "required": [ "image" ], "properties": { "auth": { "type": "object", "required": [ "password", "username" ], "properties": { "password": { "type": "string" }, "username": { "type": "string" } }, "additionalProperties": false }, "aws_auth": { "oneOf": [ { "type": "object", "required": [ "aws_access_key_id", "aws_secret_access_key" ], "properties": { "aws_access_key_id": { "type": "string" }, "aws_secret_access_key": { "type": "string" } }, "additionalProperties": false }, { "type": "object", "required": [ "oidc_role_arn" ], "properties": { "oidc_role_arn": { "type": "string" } }, "additionalProperties": false } ] }, "command": { "oneOf": [ { "type": "string" }, { "type": "array", "items": { "type": "string" } } ] }, "entrypoint": { "oneOf": [ { "type": "string" }, { "type": "array", "items": { "type": "string" } } ] }, "environment": { "oneOf": [ { "type": [ "string", "null" ], "markdownDescription": "A map of environment variable names and values." }, { "type": "array", "items": { "$ref": "#/definitions/environment" }, "markdownDescription": "An array of strings in the form KEY=VALUE" }, { "type": "object", "additionalProperties": { "type": [ "string", "boolean", "number", "null" ] }, "markdownDescription": "The value should be a string, but we allow null, boolean and numbers too. Examples: `FOO: true` and `BAR: 2` `BAZ:`" } ] }, "image": { "type": "string" }, "name": { "type": "string" }, "user": { "type": "string" } }, "additionalProperties": false }, "minItems": 1 }, "environment": { "oneOf": [ { "type": [ "string", "null" ], "markdownDescription": "A map of environment variable names and values." }, { "type": "array", "items": { "$ref": "#/definitions/environment" }, "markdownDescription": "An array of strings in the form KEY=VALUE" }, { "type": "object", "additionalProperties": { "type": [ "string", "boolean", "number", "null" ] }, "markdownDescription": "The value should be a string, but we allow null, boolean and numbers too. Examples: `FOO: true` and `BAR: 2` `BAZ:`" } ] }, "executor": { "oneOf": [ { "type": "string", "markdownDescription": "https://circleci.com/docs/reference/reusing-config/#executor\n\nThe name of the executor to use (defined via the top level executors map or from an orb)." }, { "type": "object", "minProperties": 1, "required": [ "name" ], "properties": { "name": { "type": "string" } }, "additionalProperties": true, "markdownDescription": "Executor stanza to use for the job, with optional parameter overrides." } ] }, "machine": { "oneOf": [ { "anyOf": [ { "type": "string", "pattern": " *<< *parameters.([^ ]+) *>> *" }, { "type": "boolean" }, { "type": "number" }, { "type": "string" } ] }, { "type": "object", "properties": { "docker_layer_caching": { "anyOf": [ { "type": "string", "pattern": " *<< *parameters.([^ ]+) *>> *" }, { "type": "boolean" }, { "type": "number" }, { "type": "string" } ], "markdownDescription": "Set to `true` to enable [Docker Layer Caching](https://circleci.com/docs/docker-layer-caching). Note: If you haven't already, you must open a support ticket to have a CircleCI Sales representative contact you about enabling this feature on your account for an additional fee." }, "enabled": { "anyOf": [ { "type": "string", "pattern": " *<< *parameters.([^ ]+) *>> *" }, { "type": "boolean" }, { "type": "number" }, { "type": "string" } ], "markdownDescription": "A boolean or a template parameter evaluating to a boolean" }, "image": { "type": "string" }, "resource_class": { "type": "string" }, "shell": { "type": "string" } }, "additionalProperties": false } ], "markdownDescription": "Use the default machine executor image" }, "macos": { "type": "object", "required": [ "xcode" ], "properties": { "resource_class": { "type": "string" }, "shell": { "type": "string" }, "xcode": { "type": [ "string", "number" ] } }, "additionalProperties": false }, "parallelism": { "anyOf": [ { "oneOf": [ { "type": "string", "pattern": " *<< *parameters.([^ ]+) *>> *" }, { "type": "string", "pattern": " *<< *pipeline.parameters.([^ ]+) *>> *" } ] }, { "type": "integer" } ], "markdownDescription": "A integer or a parameter evaluating to a integer" }, "parameters": { "type": "object", "minProperties": 1, "additionalProperties": { "type": "object", "required": [ "type" ], "properties": { "default": { "oneOf": [ { "type": "string" }, { "type": "boolean" }, { "type": "integer" }, { "type": "array", "items": { "$ref": "#/definitions/step" } } ] }, "description": { "type": "string", "markdownDescription": "A description of the parameter" }, "enum": { "type": "array", "items": { "type": "string" }, "minItems": 1, "markdownDescription": "List of allowed values for an `enum` type parameter" }, "type": { "type": "string", "enum": [ "boolean", "string", "steps", "enum", "executor", "integer", "env_var_name" ], "markdownDescription": "The parameter type. See [Parameter Types](https://circleci.com/docs/reusing-config#parameter-syntax) for details:\n- `boolean`: A boolean parameter\n- `string`: A string parameter \n- `integer`: An integer parameter\n- `enum`: An enum parameter (list of allowed string values)\n- `executor`: An executor parameter (allows job to choose executor)\n- `steps`: A steps parameter (allows mixing predefined and user-defined steps)\n- `env_var_name`: An environment variable name parameter (must match POSIX_NAME regexp)" } }, "additionalProperties": false }, "markdownDescription": "https://circleci.com/docs/reusing-config#using-the-parameters-declaration\n\nA map of parameter keys." }, "resource_class": { "type": "string" }, "retention": { "type": "object", "properties": { "caches": { "type": "string", "pattern": "^([1-9]\|1[0-5])d$" } }, "additionalProperties": false }, "shell": { "oneOf": [ { "type": "string" }, { "type": "array", "items": { "type": "string" } } ] }, "steps": { "type": "array", "items": { "$ref": "#/definitions/step" }, "minItems": 1 }, "type": { "const": "build", "markdownDescription": "The job type. If not specified, defaults to build." }, "working_directory": { "type": "string", "markdownDescription": "In which directory to run the steps. (default: `~/project`. `project` is a literal string, not the name of the project.) You can also refer the directory with `$CIRCLE_WORKING_DIRECTORY` environment variable." } }, "additionalProperties": false, "markdownDescription": "Validation: The job must adhere to the structure of a build job." }, "else": false } } } } |
| in-process | codegen | medium/package-json | — | — | — | FAIL: not measured: schema needs external documents (offline run) | FAIL: not measured: schema needs external documents (offline run) |
| in-process | codegen | small/prettierrc | — | — | — | FAIL: codegen error: schema at index 0: allOf subschema at index 0 is not object-like (need type "object" or non-empty properties) | FAIL: codegen error: value does not conform to the given schema |
| in-process | schema compile | medium/package-json | FAIL: not measured: schema needs external documents (offline run) | FAIL: not measured: schema needs external documents (offline run) | — | FAIL: not measured: schema needs external documents (offline run) | — |
| in-process | validate (valid) | medium/package-json | FAIL: not measured: schema needs external documents (offline run) | FAIL: not measured: schema needs external documents (offline run) | — | FAIL: not measured: schema needs external documents (offline run) | — |
| in-process | validate (invalid) | medium/package-json | FAIL: not measured: schema needs external documents (offline run) | FAIL: not measured: schema needs external documents (offline run) | — | FAIL: not measured: schema needs external documents (offline run) | — |
| CLI | codegen | large/circleci | — | — | — | pass | FAIL: exit 101: not implemented: if/then/else schemas are not supported: { |
| CLI | codegen | medium/package-json | — | — | — | FAIL: not measured: schema needs external documents (offline run) | FAIL: not measured: schema needs external documents (offline run) |
| CLI | codegen | small/prettierrc | — | — | — | FAIL: exit 1: error: schema at index 0: allOf subschema at index 0 is not object-like (need type "object" or non-empty properties) | FAIL: exit 1: value does not conform to the given schema |
| CLI | validate (valid) | medium/package-json | FAIL: not measured: schema needs external documents (offline run) | — | FAIL: not measured: schema needs external documents (offline run) | FAIL: not measured: schema needs external documents (offline run) | — |
| CLI | validate (invalid) | medium/package-json | FAIL: not measured: schema needs external documents (offline run) | — | FAIL: not measured: schema needs external documents (offline run) | FAIL: not measured: schema needs external documents (offline run) | — |
