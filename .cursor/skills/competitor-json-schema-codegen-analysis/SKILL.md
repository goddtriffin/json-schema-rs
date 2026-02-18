---
name: competitor-json-schema-codegen-analysis
description: Performs a deep-dive analysis of a single JSON Schema to codegen library from a locally cloned repo and produces a structured research report. Use when analyzing a competitor library (path to clone provided), or when generating or updating a competitor research report.
---

# Competitor library analysis (JSON Schema → codegen)

Analyze one competitor repo and write one research report. Do not compare to “our” library; describe only the competitor.

## Input

- **Clone path**: `research/repos/<lang>/<name>/` (e.g. `research/repos/rust/oxidecomputer-typify/`). Use only the cloned repo for code and docs. Do not open GitHub or the web.

## JSON Schema source

- Use **only** vendored specs under **specs/** (repository root). Never use the web for JSON Schema specification details. If specs are missing or outdated, the maintainer runs `make vendor_specs` (or fixes `specs/download.sh`).
- **Keyword table**: Derive the list of allowed keywords from the vendored meta-schemas. Do not invent keywords. Use the canonical list from `specs/json-schema.org/draft/2020-12/meta/` (and other drafts if the library targets them). See [reference.md](reference.md) for the exact meta-schema files. Optionally run `research/scripts/list_keywords.sh` if present to get the canonical keyword list.

## Output

- **Path**: `research/reports/<lang>/{org}-{repo}.md` (e.g. `research/reports/rust/oxidecomputer-typify.md`). `<lang>` must match the library’s implementation language (e.g. `rust`, `go`, `java`).
- **Format**: Follow the report structure in [reference.md](reference.md). Overwrite or create the file.

## Report sections

Produce every section from the reference template: Metadata, Summary, JSON Schema support, Keyword support table, Constraints, High-level architecture, Medium-level architecture, Low-level details (only where needed), Output and integration, Configuration, Pros/cons, Testability, Performance, Determinism and idempotency, Enum handling, Reverse generation (Schema from types), Multi-language output, Model deduplication and $ref/$defs, Validation (schema + JSON → errors). The template may be extended later (e.g. extra Performance or Testability subsections) without re-analyzing from scratch. Fill each section from repo evidence (code, docs, tests); use "Unknown" or TODO when not yet researched — do not invent answers.

**Diagrams**: Include Mermaid diagrams in High-level architecture and Medium-level architecture (and elsewhere if helpful) to show pipeline, ref resolution, or expansion flow. See [reference.md](reference.md) for diagram conventions (node IDs without spaces, quoted labels for special characters, no HTML or explicit styling).

**Report purpose**: Reports are standalone; use them first. Fall back to the cloned repo only when deeper implementation detail is needed.

## Rules

- No comparison to this repo’s implementation. Describe the competitor only.
- All code and documentation evidence must come from the cloned repo at the given path.
- One row per keyword in the keyword table; columns Keyword, Implemented (yes/no/partial), Notes.
