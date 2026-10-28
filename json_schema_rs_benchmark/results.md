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
| in-process | schema compile | peak heap | jsonschemars | 10 / 11 |
| in-process | validate (valid) | time | jsonschema | 11 / 11 |
| in-process | validate (valid) | peak heap | jsonschema | 11 / 11 |
| in-process | validate (invalid) | time | jsonschema | 9 / 11 |
| in-process | validate (invalid) | peak heap | jsonschema | 9 / 11 |
| CLI | codegen | time | jsonschemars | 9 / 9 |
| CLI | codegen | peak RSS | jsonschemars | 6 / 9 |
| CLI | validate (valid) | time | jsonschemars | 9 / 11 |
| CLI | validate (valid) | peak RSS | jsonschemars | 10 / 11 |
| CLI | validate (invalid) | time | jsonschemars | 9 / 11 |
| CLI | validate (invalid) | peak RSS | jsonschemars | 10 / 11 |

## In-process (library calls)

### codegen — time

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | **6.199 ± 0.074 ms** | 9.638 ± 0.068 ms |
| generated/large-array | **72.927 ± 0.075 µs** | 1.025 ± 0.008 ms |
| generated/many-defs | **37.121 ± 0.429 ms** | 122.973 ± 2.393 ms |
| generated/wide-object | **137.401 ± 0.670 ms** | 147.089 ± 0.284 ms |
| large/api-catalog | **204.869 ± 0.191 µs** | 3.091 ± 0.002 ms |
| large/circleci | 1.373 ± 0.002 ms | — |
| massive/data-warehouse | **9.373 ± 0.077 ms** | 269.240 ± 2.235 ms |
| massive/k8s-deployment | 18.659 ± 0.092 ms | **1.816 ± 0.005 ms** |
| medium/user-profile | **91.918 ± 0.164 µs** | 1.421 ± 0.002 ms |
| small/config | **29.439 ± 0.052 µs** | 509.995 ± 0.759 µs |

### codegen — peak heap

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | 36.00 MiB | **12.65 MiB** |
| generated/large-array | **503.70 KiB** | 1.23 MiB |
| generated/many-defs | **93.09 MiB** | 133.38 MiB |
| generated/wide-object | **31.36 MiB** | 162.36 MiB |
| large/api-catalog | **1.12 MiB** | 3.64 MiB |
| large/circleci | 7.80 MiB | — |
| massive/data-warehouse | **29.33 MiB** | 288.90 MiB |
| massive/k8s-deployment | 78.20 MiB | **2.79 MiB** |
| medium/user-profile | **453.59 KiB** | 1.71 MiB |
| small/config | **133.13 KiB** | 639.58 KiB |

### schema compile — time

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 1.024 ± 0.014 ms | 206.653 ± 0.592 µs | **64.521 ± 0.140 µs** |
| generated/large-array | 45.332 ± 0.681 µs | 20.895 ± 0.140 µs | **4.357 ± 0.009 µs** |
| generated/many-defs | 13.769 ± 0.019 ms | 3.009 ± 0.046 ms | **703.782 ± 10.073 µs** |
| generated/wide-object | 23.166 ± 0.047 ms | 2.522 ± 0.003 ms | **765.150 ± 0.961 µs** |
| large/api-catalog | 177.394 ± 0.162 µs | 102.364 ± 0.091 µs | **15.584 ± 0.075 µs** |
| large/circleci | 2.857 ± 0.014 ms | 1.016 ± 0.002 ms | **254.795 ± 0.613 µs** |
| massive/data-warehouse | 44.260 ± 0.188 ms | 6.298 ± 0.022 ms | **1.270 ± 0.003 ms** |
| massive/k8s-deployment | 7.336 ± 0.014 ms | 1.604 ± 0.005 ms | **609.997 ± 2.736 µs** |
| medium/user-profile | 66.774 ± 0.154 µs | 40.253 ± 0.210 µs | **5.690 ± 0.011 µs** |
| small/config | 26.822 ± 0.061 µs | 18.912 ± 0.077 µs | **1.838 ± 0.006 µs** |
| small/prettierrc | 203.904 ± 0.262 µs | 110.654 ± 0.087 µs | **25.350 ± 0.041 µs** |

### schema compile — peak heap

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 636.04 KiB | 534.41 KiB | **293.28 KiB** |
| generated/large-array | 66.67 KiB | 34.10 KiB | **30.66 KiB** |
| generated/many-defs | 8.10 MiB | **3.88 MiB** | 4.16 MiB |
| generated/wide-object | 13.73 MiB | 4.03 MiB | **2.67 MiB** |
| large/api-catalog | 266.71 KiB | 151.24 KiB | **86.06 KiB** |
| large/circleci | 3.85 MiB | 1.75 MiB | **704.83 KiB** |
| massive/data-warehouse | 16.73 MiB | 9.42 MiB | **2.91 MiB** |
| massive/k8s-deployment | 7.45 MiB | 3.28 MiB | **2.18 MiB** |
| medium/user-profile | 121.82 KiB | 59.13 KiB | **30.83 KiB** |
| small/config | 38.35 KiB | 24.08 KiB | **7.90 KiB** |
| small/prettierrc | 476.85 KiB | 261.04 KiB | **123.03 KiB** |

### validate (valid) — time

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.918 ± 0.022 µs | **2.387 ± 0.021 µs** | 138.962 ± 0.225 µs |
| generated/large-array | 18.041 ± 0.069 ms | **7.464 ± 0.090 ms** | 148.152 ± 0.204 ms |
| generated/many-defs | 1.746 ± 0.013 ms | **621.262 ± 0.977 µs** | 18.917 ± 0.124 ms |
| generated/wide-object | 169.231 ± 0.380 µs | **112.857 ± 2.052 µs** | 783.447 ± 1.226 µs |
| large/api-catalog | 562.487 ± 1.452 ns | **242.709 ± 0.309 ns** | 6.670 ± 0.036 µs |
| large/circleci | 2.164 ± 0.008 µs | **466.715 ± 1.104 ns** | 2.182 ± 0.006 µs |
| massive/data-warehouse | 69.423 ± 0.145 µs | **23.672 ± 0.042 µs** | 605.697 ± 4.218 µs |
| massive/k8s-deployment | 517.143 ± 1.914 ns | **213.566 ± 0.815 ns** | 4.613 ± 0.019 µs |
| medium/user-profile | 839.027 ± 3.506 ns | **369.661 ± 1.422 ns** | 5.829 ± 0.016 µs |
| small/config | 227.895 ± 0.684 ns | **113.843 ± 0.549 ns** | 2.733 ± 0.013 µs |
| small/prettierrc | 726.315 ± 6.775 ns | **175.317 ± 0.377 ns** | 3.081 ± 0.010 µs |

### validate (valid) — peak heap

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 2.25 KiB | **0.03 KiB** | 11.74 KiB |
| generated/large-array | 0.19 KiB | **0.03 KiB** | 8.47 MiB |
| generated/many-defs | 0.56 KiB | **0.03 KiB** | 16.86 KiB |
| generated/wide-object | 0.19 KiB | **0.03 KiB** | 548.52 KiB |
| large/api-catalog | 0.19 KiB | **0.03 KiB** | 2.95 KiB |
| large/circleci | 2.58 KiB | **0.03 KiB** | 1.25 KiB |
| massive/data-warehouse | 0.19 KiB | **0.03 KiB** | 14.77 KiB |
| massive/k8s-deployment | 0.19 KiB | **0.03 KiB** | 1.56 KiB |
| medium/user-profile | 0.19 KiB | **0.03 KiB** | 3.48 KiB |
| small/config | 0.19 KiB | **0.03 KiB** | 2.96 KiB |
| small/prettierrc | 0.77 KiB | **0.03 KiB** | 3.78 KiB |

### validate (invalid) — time

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 7.680 ± 0.009 µs | **2.783 ± 0.024 µs** | 139.912 ± 0.376 µs |
| generated/large-array | 18.008 ± 0.044 ms | **7.418 ± 0.038 ms** | 147.093 ± 1.254 ms |
| generated/many-defs | 1.767 ± 0.036 ms | **622.709 ± 1.374 µs** | 18.821 ± 0.043 ms |
| generated/wide-object | 168.865 ± 0.869 µs | **112.513 ± 0.439 µs** | 796.345 ± 6.423 µs |
| large/api-catalog | 652.445 ± 0.918 ns | **391.108 ± 1.000 ns** | 5.325 ± 0.052 µs |
| large/circleci | **125.753 ± 0.233 ns** | 130.168 ± 0.490 ns | 403.637 ± 0.909 ns |
| massive/data-warehouse | 180.892 ± 2.871 µs | **138.507 ± 0.336 µs** | 889.359 ± 5.195 µs |
| massive/k8s-deployment | 180.960 ± 3.017 ns | **134.540 ± 3.227 ns** | 564.563 ± 6.182 ns |
| medium/user-profile | 368.528 ± 1.263 ns | **310.672 ± 0.351 ns** | 2.352 ± 0.007 µs |
| small/config | 288.067 ± 1.267 ns | **244.516 ± 0.414 ns** | 2.753 ± 0.005 µs |
| small/prettierrc | **519.820 ± 1.497 ns** | 897.692 ± 6.020 ns | 882.914 ± 3.414 ns |

### validate (invalid) — peak heap

| Fixture | boon | jsonschema | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 2.97 KiB | **0.75 KiB** | 11.74 KiB |
| generated/large-array | 1.37 KiB | **0.46 KiB** | 8.47 MiB |
| generated/many-defs | 7.77 KiB | **0.77 KiB** | 16.86 KiB |
| generated/wide-object | 0.74 KiB | **0.41 KiB** | 548.52 KiB |
| large/api-catalog | 1.44 KiB | **0.83 KiB** | 3.12 KiB |
| large/circleci | 0.74 KiB | **0.40 KiB** | 1.22 KiB |
| massive/data-warehouse | 467.89 KiB | 111.78 KiB | **4.59 KiB** |
| massive/k8s-deployment | 0.81 KiB | **0.48 KiB** | 1.07 KiB |
| medium/user-profile | 1.30 KiB | **0.73 KiB** | 1.85 KiB |
| small/config | 1.30 KiB | **0.71 KiB** | 4.06 KiB |
| small/prettierrc | 2.38 KiB | 2.69 KiB | **1.55 KiB** |

## CLI (end to end)

### codegen — time

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | **11.031 ± 0.308 ms** | 107.528 ± 0.337 ms |
| generated/large-array | **1.823 ± 0.058 ms** | 86.594 ± 0.768 ms |
| generated/many-defs | **43.977 ± 0.377 ms** | 441.815 ± 2.015 ms |
| generated/wide-object | **142.853 ± 1.239 ms** | 512.507 ± 3.271 ms |
| large/api-catalog | **2.113 ± 0.038 ms** | 93.404 ± 0.688 ms |
| large/circleci | 3.973 ± 0.064 ms | — |
| massive/data-warehouse | **14.022 ± 0.755 ms** | 785.350 ± 3.555 ms |
| massive/k8s-deployment | **27.368 ± 0.164 ms** | 85.436 ± 0.693 ms |
| medium/user-profile | **1.815 ± 0.041 ms** | 87.060 ± 0.670 ms |
| small/config | **1.774 ± 0.118 ms** | 84.367 ± 1.781 ms |

### codegen — peak RSS

| Fixture | jsonschemars | typify |
|---|---|---|
| generated/deep-nesting | 45.42 MiB | **38.00 ± 0.02 MiB** |
| generated/large-array | **3.77 ± 0.01 MiB** | 33.31 ± 0.01 MiB |
| generated/many-defs | 115.05 MiB | **98.14 ± 0.04 MiB** |
| generated/wide-object | **38.67 ± 0.01 MiB** | 115.14 ± 0.05 MiB |
| large/api-catalog | **4.88 ± 0.01 MiB** | 34.72 ± 0.04 MiB |
| large/circleci | 12.67 ± 0.00 MiB | — |
| massive/data-warehouse | **37.58 ± 0.32 MiB** | 177.31 MiB |
| massive/k8s-deployment | 95.94 ± 0.04 MiB | **31.77 MiB** |
| medium/user-profile | **3.83 MiB** | 33.59 MiB |
| small/config | **3.33 MiB** | 32.92 MiB |

### validate (valid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.358 ± 0.423 ms | 4.068 ± 0.235 ms | **1.868 ± 0.043 ms** |
| generated/large-array | 58.615 ± 0.444 ms | **44.457 ± 0.644 ms** | 187.584 ± 1.679 ms |
| generated/many-defs | 22.934 ± 0.265 ms | **11.297 ± 0.148 ms** | 26.151 ± 0.385 ms |
| generated/wide-object | 27.848 ± 0.429 ms | 8.253 ± 0.717 ms | **4.020 ± 0.039 ms** |
| large/api-catalog | 3.019 ± 0.073 ms | 3.798 ± 0.122 ms | **1.647 ± 0.078 ms** |
| large/circleci | 6.730 ± 0.111 ms | 5.688 ± 0.255 ms | **1.923 ± 0.018 ms** |
| massive/data-warehouse | 50.067 ± 0.326 ms | 11.811 ± 0.120 ms | **4.175 ± 0.040 ms** |
| massive/k8s-deployment | 11.756 ± 0.204 ms | 289.361 ± 37.773 ms † | **2.554 ± 0.038 ms** |
| medium/user-profile | 2.854 ± 0.102 ms | 3.695 ± 0.088 ms | **1.508 ± 0.026 ms** |
| small/config | 2.855 ± 0.057 ms | 3.712 ± 0.098 ms | **1.510 ± 0.047 ms** |
| small/prettierrc | 3.005 ± 0.195 ms | 3.741 ± 0.093 ms | **1.600 ± 0.064 ms** |

### validate (valid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.91 MiB | 9.89 ± 0.01 MiB | **4.03 MiB** |
| generated/large-array | **55.77 MiB** | 64.67 MiB | 68.25 ± 0.00 MiB |
| generated/many-defs | 19.06 ± 0.00 MiB | 21.36 MiB | **14.28 MiB** |
| generated/wide-object | 15.11 ± 0.00 MiB | 15.52 ± 0.10 MiB | **7.03 ± 0.01 MiB** |
| large/api-catalog | 5.86 MiB | 10.36 ± 0.01 MiB | **3.14 MiB** |
| large/circleci | 9.12 ± 0.04 MiB | 13.09 ± 0.01 MiB | **4.30 MiB** |
| massive/data-warehouse | 23.62 MiB | 20.92 MiB | **7.61 MiB** |
| massive/k8s-deployment | 11.78 MiB | 20.44 ± 0.05 MiB | **6.22 MiB** |
| medium/user-profile | 5.70 ± 0.01 MiB | 10.06 MiB | **3.06 MiB** |
| small/config | 5.69 MiB | 9.86 MiB | **3.00 MiB** |
| small/prettierrc | 5.86 MiB | 8.97 ± 0.01 MiB | **3.16 ± 0.01 MiB** |

### validate (invalid) — time

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 4.086 ± 0.110 ms | 3.973 ± 0.062 ms | **1.911 ± 0.087 ms** |
| generated/large-array | 58.405 ± 0.192 ms | **44.257 ± 0.404 ms** | 187.107 ± 2.163 ms |
| generated/many-defs | 23.232 ± 0.289 ms | **11.401 ± 0.129 ms** | 26.404 ± 0.535 ms |
| generated/wide-object | 27.778 ± 0.120 ms | 8.005 ± 0.066 ms | **4.011 ± 0.119 ms** |
| large/api-catalog | 3.018 ± 0.147 ms | 3.962 ± 0.142 ms | **1.538 ± 0.026 ms** |
| large/circleci | 6.744 ± 0.138 ms | 5.546 ± 0.084 ms | **1.966 ± 0.079 ms** |
| massive/data-warehouse | 51.074 ± 0.414 ms | 11.828 ± 0.312 ms | **4.419 ± 0.214 ms** |
| massive/k8s-deployment | 11.867 ± 0.108 ms | 287.015 ± 39.691 ms † | **2.556 ± 0.138 ms** |
| medium/user-profile | 2.846 ± 0.129 ms | 3.618 ± 0.062 ms | **1.496 ± 0.031 ms** |
| small/config | 2.797 ± 0.172 ms | 3.678 ± 0.087 ms | **1.490 ± 0.039 ms** |
| small/prettierrc | 3.028 ± 0.039 ms | 3.618 ± 0.060 ms | **1.538 ± 0.023 ms** |

### validate (invalid) — peak RSS

| Fixture | boon | jsonschema-cli | jsonschemars |
|---|---|---|---|
| generated/deep-nesting | 6.91 MiB | 10.03 ± 0.02 MiB | **4.06 MiB** |
| generated/large-array | **55.78 ± 0.01 MiB** | 64.84 MiB | 68.28 ± 0.00 MiB |
| generated/many-defs | 19.08 ± 0.01 MiB | 21.44 MiB | **14.38 MiB** |
| generated/wide-object | 15.08 ± 0.01 MiB | 15.39 ± 0.01 MiB | **7.05 ± 0.01 MiB** |
| large/api-catalog | 5.91 ± 0.02 MiB | 10.48 ± 0.01 MiB | **3.16 MiB** |
| large/circleci | 9.08 ± 0.02 MiB | 13.12 ± 0.05 MiB | **4.27 ± 0.01 MiB** |
| massive/data-warehouse | 24.27 MiB | 21.08 ± 0.02 MiB | **7.42 ± 0.04 MiB** |
| massive/k8s-deployment | 11.77 MiB | 20.50 ± 0.01 MiB | **6.22 ± 0.01 MiB** |
| medium/user-profile | 5.72 MiB | 10.22 MiB | **3.08 MiB** |
| small/config | 5.72 MiB | 10.02 ± 0.01 MiB | **3.03 MiB** |
| small/prettierrc | 5.88 ± 0.01 MiB | 9.12 ± 0.00 MiB | **3.16 MiB** |

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
