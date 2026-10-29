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
| generated/deep-nesting | **4.264 ± 0.012 ms** | 9.124 ± 0.030 ms |
| generated/large-array | **59.847 ± 0.195 µs** | 981.897 ± 1.634 µs |
| generated/many-defs | **25.851 ± 0.285 ms** | 117.201 ± 1.844 ms |
| generated/wide-object | **129.429 ± 0.632 ms** | 141.575 ± 0.411 ms |
| large/api-catalog | **189.226 ± 0.243 µs** | 2.997 ± 0.004 ms |
| large/circleci | 1.593 ± 0.002 ms | — |
| massive/data-warehouse | **7.534 ± 0.011 ms** | 261.735 ± 1.583 ms |
| massive/k8s-deployment | 14.322 ± 0.110 ms | **1.794 ± 0.002 ms** |
| medium/user-profile | **78.803 ± 0.471 µs** | 1.401 ± 0.011 ms |
| small/config | **26.507 ± 0.086 µs** | 507.475 ± 1.230 µs |

### codegen — peak heap

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | 19.03 MiB | **12.65 MiB** |
| generated/large-array | **250.32 KiB** | 1.23 MiB |
| generated/many-defs | **41.42 MiB** | 133.38 MiB |
| generated/wide-object | **19.36 MiB** | 162.36 MiB |
| large/api-catalog | **667.00 KiB** | 3.64 MiB |
| large/circleci | 5.90 MiB | — |
| massive/data-warehouse | **24.57 MiB** | 288.90 MiB |
| massive/k8s-deployment | 47.59 MiB | **2.79 MiB** |
| medium/user-profile | **247.89 KiB** | 1.71 MiB |
| small/config | **95.47 KiB** | 639.58 KiB |

### schema compile — time

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 994.715 ± 21.023 µs | 205.474 ± 0.524 µs | **68.103 ± 0.106 µs** |
| generated/large-array | 45.015 ± 0.237 µs | 20.820 ± 0.157 µs | **5.967 ± 0.025 µs** |
| generated/many-defs | 13.704 ± 0.098 ms | 3.017 ± 0.004 ms | **1.016 ± 0.019 ms** |
| generated/wide-object | 23.343 ± 0.077 ms | 2.514 ± 0.006 ms | **806.202 ± 1.118 µs** |
| large/api-catalog | 178.221 ± 0.142 µs | 103.037 ± 0.352 µs | **22.100 ± 0.050 µs** |
| large/circleci | 2.870 ± 0.015 ms | 1.011 ± 0.001 ms | **299.550 ± 1.659 µs** |
| massive/data-warehouse | 44.384 ± 0.947 ms | 6.306 ± 0.038 ms | **1.502 ± 0.009 ms** |
| massive/k8s-deployment | 7.416 ± 0.021 ms | 1.600 ± 0.004 ms | **493.134 ± 0.586 µs** |
| medium/user-profile | 67.113 ± 0.070 µs | 40.279 ± 0.073 µs | **8.508 ± 0.023 µs** |
| small/config | 27.085 ± 0.099 µs | 18.919 ± 0.046 µs | **3.034 ± 0.011 µs** |
| small/prettierrc | 204.998 ± 0.278 µs | 110.917 ± 0.443 µs | **32.382 ± 0.030 µs** |

### schema compile — peak heap

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 636.04 KiB | 534.41 KiB | **117.59 KiB** |
| generated/large-array | 66.67 KiB | 34.10 KiB | **16.83 KiB** |
| generated/many-defs | 8.10 MiB | 3.88 MiB | **2.30 MiB** |
| generated/wide-object | 13.73 MiB | 4.03 MiB | **2.18 MiB** |
| large/api-catalog | 266.71 KiB | 151.24 KiB | **51.75 KiB** |
| large/circleci | 3.85 MiB | 1.75 MiB | **737.55 KiB** |
| massive/data-warehouse | 16.73 MiB | 9.42 MiB | **3.37 MiB** |
| massive/k8s-deployment | 7.45 MiB | 3.28 MiB | **1.22 MiB** |
| medium/user-profile | 121.82 KiB | 59.13 KiB | **21.96 KiB** |
| small/config | 38.35 KiB | 24.08 KiB | **7.86 KiB** |
| small/prettierrc | 476.85 KiB | 261.04 KiB | **97.37 KiB** |

### validate (valid) — time

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.686 ± 0.018 µs | **2.396 ± 0.025 µs** | 138.610 ± 0.173 µs |
| generated/large-array | 18.004 ± 0.080 ms | **7.299 ± 0.050 ms** | 142.372 ± 0.945 ms |
| generated/many-defs | 1.709 ± 0.011 ms | **627.564 ± 4.135 µs** | 18.799 ± 0.182 ms |
| generated/wide-object | 168.844 ± 0.389 µs | **111.122 ± 0.460 µs** | 657.504 ± 5.776 µs |
| large/api-catalog | 564.657 ± 1.945 ns | **238.742 ± 0.839 ns** | 6.646 ± 0.012 µs |
| large/circleci | 2.182 ± 0.018 µs | **464.377 ± 2.976 ns** | 2.047 ± 0.003 µs |
| massive/data-warehouse | 69.952 ± 0.114 µs | **23.829 ± 0.042 µs** | 617.227 ± 2.404 µs |
| massive/k8s-deployment | 520.384 ± 1.704 ns | **214.501 ± 0.454 ns** | 4.650 ± 0.014 µs |
| medium/user-profile | 864.084 ± 6.098 ns | **375.786 ± 2.154 ns** | 5.874 ± 0.015 µs |
| small/config | 224.578 ± 0.358 ns | **112.222 ± 0.369 ns** | 2.768 ± 0.007 µs |
| small/prettierrc | 723.013 ± 3.759 ns | **172.878 ± 0.457 ns** | 2.889 ± 0.014 µs |

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
| generated/deep-nesting | 7.795 ± 0.017 µs | **2.817 ± 0.006 µs** | 139.375 ± 0.253 µs |
| generated/large-array | 17.956 ± 0.076 ms | **7.333 ± 0.040 ms** | 142.696 ± 1.928 ms |
| generated/many-defs | 1.731 ± 0.005 ms | **629.048 ± 1.144 µs** | 18.653 ± 0.061 ms |
| generated/wide-object | 169.000 ± 0.472 µs | **110.316 ± 2.100 µs** | 656.447 ± 3.204 µs |
| large/api-catalog | 653.666 ± 0.951 ns | **395.743 ± 2.296 ns** | 5.341 ± 0.017 µs |
| large/circleci | 135.761 ± 0.175 ns | **129.730 ± 0.629 ns** | 407.647 ± 0.902 ns |
| massive/data-warehouse | 179.099 ± 0.437 µs | **137.685 ± 0.267 µs** | 901.250 ± 10.211 µs |
| massive/k8s-deployment | 186.423 ± 0.277 ns | **136.586 ± 0.701 ns** | 573.593 ± 2.373 ns |
| medium/user-profile | 366.974 ± 0.326 ns | **319.406 ± 1.215 ns** | 2.385 ± 0.014 µs |
| small/config | 286.965 ± 2.084 ns | **243.516 ± 0.425 ns** | 2.813 ± 0.004 µs |
| small/prettierrc | **528.377 ± 2.608 ns** | 889.075 ± 1.564 ns | 894.270 ± 3.731 ns |

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
| generated/deep-nesting | **8.354 ± 0.532 ms** | 106.891 ± 1.861 ms |
| generated/large-array | **1.764 ± 0.064 ms** | 84.980 ± 0.564 ms |
| generated/many-defs | **32.292 ± 0.293 ms** | 434.423 ± 1.364 ms |
| generated/wide-object | **138.108 ± 2.001 ms** | 504.486 ± 1.617 ms |
| large/api-catalog | **2.116 ± 0.034 ms** | 92.589 ± 0.806 ms |
| large/circleci | 4.141 ± 0.025 ms | — |
| massive/data-warehouse | **11.899 ± 0.104 ms** | 781.720 ± 3.113 ms |
| massive/k8s-deployment | **22.331 ± 0.105 ms** | 84.421 ± 0.547 ms |
| medium/user-profile | **1.835 ± 0.041 ms** | 84.806 ± 0.501 ms |
| small/config | **1.712 ± 0.034 ms** | 82.223 ± 0.707 ms |

### codegen — peak RSS

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | **24.14 ± 0.02 MiB** | 37.98 ± 0.03 MiB |
| generated/large-array | **3.77 MiB** | 33.30 ± 0.01 MiB |
| generated/many-defs | **56.89 MiB** | 98.03 MiB |
| generated/wide-object | **24.89 ± 0.02 MiB** | 114.64 ± 0.31 MiB |
| large/api-catalog | **4.66 ± 0.02 MiB** | 34.66 ± 0.01 MiB |
| large/circleci | 10.69 MiB | — |
| massive/data-warehouse | **32.62 ± 0.00 MiB** | 176.03 ± 0.06 MiB |
| massive/k8s-deployment | 57.48 ± 0.03 MiB | **31.78 ± 0.00 MiB** |
| medium/user-profile | **3.86 MiB** | 33.58 ± 0.02 MiB |
| small/config | **3.55 ± 0.01 MiB** | 32.94 MiB |

### validate (valid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.471 ± 0.496 ms † | 3.889 ± 0.055 ms | **1.865 ± 0.036 ms** |
| generated/large-array | 58.703 ± 0.392 ms | **44.402 ± 0.365 ms** | 187.281 ± 0.785 ms |
| generated/many-defs | 22.956 ± 0.274 ms | **11.143 ± 0.088 ms** | 25.931 ± 0.253 ms |
| generated/wide-object | 27.646 ± 0.199 ms | 7.898 ± 0.077 ms | **3.807 ± 0.038 ms** |
| large/api-catalog | 2.989 ± 0.047 ms | 3.784 ± 0.049 ms | **1.625 ± 0.035 ms** |
| large/circleci | 6.641 ± 0.057 ms | 5.481 ± 0.062 ms | **2.047 ± 0.020 ms** |
| massive/data-warehouse | 50.107 ± 0.801 ms | 11.779 ± 0.489 ms | **4.488 ± 0.061 ms** |
| massive/k8s-deployment | 11.458 ± 0.135 ms | 446.337 ± 168.866 ms † | **2.366 ± 0.034 ms** |
| medium/user-profile | 2.806 ± 0.031 ms | 3.590 ± 0.036 ms | **1.573 ± 0.041 ms** |
| small/config | 2.745 ± 0.027 ms | 3.519 ± 0.037 ms | **1.535 ± 0.037 ms** |
| small/prettierrc | 2.958 ± 0.044 ms | 3.589 ± 0.038 ms | **1.584 ± 0.020 ms** |

### validate (valid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.77 MiB | 9.88 ± 0.01 MiB | **3.91 MiB** |
| generated/large-array | **55.78 MiB** | 64.66 ± 0.01 MiB | 67.27 ± 0.01 MiB |
| generated/many-defs | 19.05 MiB | 21.38 MiB | **12.27 ± 0.01 MiB** |
| generated/wide-object | 15.08 MiB | 15.27 ± 0.01 MiB | **6.59 MiB** |
| large/api-catalog | 5.84 MiB | 10.36 ± 0.01 MiB | **3.36 ± 0.00 MiB** |
| large/circleci | 9.05 MiB | 13.06 ± 0.01 MiB | **4.44 MiB** |
| massive/data-warehouse | 22.00 ± 0.02 MiB | 20.98 ± 0.01 MiB | **8.23 ± 0.01 MiB** |
| massive/k8s-deployment | 11.77 MiB | 20.53 ± 0.03 MiB | **5.16 ± 0.01 MiB** |
| medium/user-profile | 5.72 MiB | 10.09 ± 0.01 MiB | **3.30 MiB** |
| small/config | 5.70 ± 0.02 MiB | 9.86 ± 0.01 MiB | **3.22 MiB** |
| small/prettierrc | 5.84 ± 0.01 MiB | 8.95 MiB | **3.33 MiB** |

### validate (invalid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 3.924 ± 0.037 ms | 3.900 ± 0.048 ms | **1.877 ± 0.026 ms** |
| generated/large-array | 58.515 ± 0.293 ms | **44.413 ± 0.369 ms** | 187.040 ± 1.468 ms |
| generated/many-defs | 23.018 ± 0.342 ms | **11.180 ± 0.063 ms** | 25.906 ± 0.255 ms |
| generated/wide-object | 27.600 ± 0.102 ms | 7.770 ± 0.082 ms | **3.793 ± 0.042 ms** |
| large/api-catalog | 2.992 ± 0.040 ms | 3.772 ± 0.035 ms | **1.611 ± 0.031 ms** |
| large/circleci | 6.584 ± 0.035 ms | 5.451 ± 0.047 ms | **2.024 ± 0.027 ms** |
| massive/data-warehouse | 51.156 ± 1.285 ms | 11.826 ± 0.125 ms | **4.697 ± 0.038 ms** |
| massive/k8s-deployment | 11.503 ± 0.142 ms | 560.930 ± 228.553 ms † | **2.355 ± 0.042 ms** |
| medium/user-profile | 2.812 ± 0.028 ms | 3.723 ± 0.167 ms | **1.533 ± 0.029 ms** |
| small/config | 2.756 ± 0.027 ms | 3.490 ± 0.040 ms | **1.511 ± 0.027 ms** |
| small/prettierrc | 2.976 ± 0.031 ms | 3.565 ± 0.060 ms | **1.561 ± 0.030 ms** |

### validate (invalid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.72 ± 0.02 MiB | 10.03 ± 0.01 MiB | **4.02 MiB** |
| generated/large-array | **55.78 MiB** | 64.80 ± 0.01 MiB | 67.38 MiB |
| generated/many-defs | 19.03 MiB | 21.44 ± 0.02 MiB | **12.38 ± 0.01 MiB** |
| generated/wide-object | 15.09 ± 0.01 MiB | 15.41 ± 0.01 MiB | **6.69 ± 0.01 MiB** |
| large/api-catalog | 5.88 MiB | 10.50 ± 0.01 MiB | **3.41 ± 0.01 MiB** |
| large/circleci | 9.05 ± 0.02 MiB | 13.08 ± 0.04 MiB | **4.55 MiB** |
| massive/data-warehouse | 23.05 ± 0.13 MiB | 21.06 ± 0.01 MiB | **8.09 MiB** |
| massive/k8s-deployment | 11.78 ± 0.03 MiB | 20.59 ± 0.07 MiB | **5.20 MiB** |
| medium/user-profile | 5.73 MiB | 10.22 MiB | **3.33 MiB** |
| small/config | 5.70 MiB | 10.02 MiB | **3.31 MiB** |
| small/prettierrc | 5.83 ± 0.03 MiB | 9.14 ± 0.01 MiB | **3.36 ± 0.01 MiB** |

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
