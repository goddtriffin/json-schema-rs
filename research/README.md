# Competitor research

This directory holds cloned competitor repos, research reports, benchmark fixtures, and test-harvest layout for JSON Schema → codegen libraries.

- **repos/** — Cloned repos by language (e.g. `repos/rust/oxidecomputer-typify/`). Filled by `make research-clone`. Gitignored.
- **reports/** — One Markdown report per library (e.g. `reports/rust/oxidecomputer-typify.md`). Generated using the competitor analysis Skill.
- **benchmark/** — Shared JSON Schema fixtures and (future) harness for measuring performance. See `benchmark/README.md`.
- **test-harvest/** — (Future) Harvested test inputs from other repos. See `test-harvest/README.md`.

JSON Schema specifications are vendored in the repo under **specs/** (not here). To refresh them run:

```bash
make vendor-specs
```

from the repository root.
