# Test harvest (design)

**Purpose**: Run our generator on every schema that other libraries use in their tests, and on all JSON Schema spec test cases, so we don’t regress and we support at least what others support. Our goal is the highest-quality unit test suite: 100% of JSON Schema spec tests plus all competitor test inputs; our library must succeed on all (for the subset that is “generate Rust structs”).

**Plan**:

1. **Identify** test schemas in each cloned repo under `research/repos/<lang>/<name>/` (e.g. `research/repos/rust/oxidecomputer-typify/`).
2. **Copy or symlink** them into a unified structure, e.g. `research/test-harvest/schemas/<lang>/<source>/…` (e.g. `schemas/rust/oxidecomputer-typify/…`).
3. **Run** our test suite against each harvested schema: our generator must succeed (and optionally we assert on output shape).

No copying or harvest scripts are implemented in the initial phase; only this README and the directory layout. Implement when ready.

**Makefile**: The target `make research-harvest-tests` exists as a stub and will exit with “Not implemented” until the harvest and test runner are added.
