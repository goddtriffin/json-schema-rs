# Competitor research

This directory holds cloned competitor repos, research reports, benchmark fixtures, and test-harvest layout for JSON Schema → codegen libraries.

- **repos/** — Cloned repos by language (e.g. `repos/rust/oxidecomputer-typify/`). Filled by `make research_clone` (Git clones plus the BelfordZ json_schema crate from crates.io). Use `REFRESH=1 make research_clone` to force re-download of the BelfordZ crate (the script removes it automatically). Languages: Rust, Python, Go, Java, TypeScript, C, C++, Zig, C#, PHP, Kotlin, Lua, Ruby, Dart, Swift. Quicktype is cloned once under TypeScript for shared use. Gitignored.
- **reports/** — One Markdown report per library (e.g. `reports/rust/oxidecomputer-typify.md`). Generated using the competitor analysis Skill.
- **benchmark/** — Shared JSON Schema fixtures and (future) harness for measuring performance. See `benchmark/README.md`.
- **test-harvest/** — (Future) Harvested test inputs from other repos. See `test-harvest/README.md`.

JSON Schema specifications are **downloaded** via `make vendor_specs` (not stored in the repo). They appear under **specs/** after you run that command from the repository root. Report wording like "vendored draft 2020-12 meta-schemas" refers to this same local specs directory populated by the download script.
