# Competitor research

This directory holds cloned competitor repos, research reports, and test-harvest layout for JSON Schema → codegen libraries.

- **repos/** — Cloned repos by language (e.g. `repos/rust/oxidecomputer-typify/`). Filled by `make research_clone` (Git clones plus the BelfordZ json_schema crate from crates.io). Use `REFRESH=1 make research_clone` to force re-download of the BelfordZ crate (the script removes it automatically). Languages: Rust, Python, Go, Java, TypeScript, C, C++, Zig, C#, PHP, Kotlin, Lua, Ruby, Dart, Swift. Quicktype is cloned once under TypeScript for shared use. Gitignored.
- **reports/** — One Markdown report per library (e.g. `reports/rust/oxidecomputer-typify.md`). Generated using the competitor analysis Skill.
- **synthesis.md** — Cross-competitor synthesis: one section per design concern (ref resolution, compiled form, error model, dedup, enum naming, performance techniques, …) summarizing how the deeply analyzed competitors approach it and what json-schema-rs should take from each. Built from the reports, not from the clones.

### Refreshing reports

1. `make vendor_specs`, then confirm `specs/json-schema.org/draft/2020-12/meta/` contains the meta-schema JSON files (the directory can exist with only a placeholder).
2. `make research_clone` **once** — it resets every clone to the latest upstream commit and resolves the latest BelfordZ crate from crates.io. Nothing is pinned.
3. One subagent per competitor runs the **competitor-json-schema-codegen-analysis** skill against its clone. A refresh is always a full deep re-analysis (every section re-read against the code; algorithms, data structures, and performance techniques at mechanism depth), never a metadata or changelog touch-up. Agents read code only: no fetching, building, executing, or benchmarking.
4. Update `synthesis.md` sections affected by changed reports.
- **test-harvest/** — (Future) Harvested test inputs from other repos. See `test-harvest/README.md`.

The performance benchmark harness (shared JSON Schema fixtures, runner, and results) lives in the top-level **`json_schema_rs_benchmark/`** workspace crate, next to `json_schema_rs/`. Run it with `make benchmark`; see `json_schema_rs_benchmark/README.md`.

JSON Schema specifications are **downloaded** via `make vendor_specs` (not stored in the repo). They appear under **specs/** after you run that command from the repository root. Report wording like "vendored draft 2020-12 meta-schemas" refers to this same local specs directory populated by the download script.
