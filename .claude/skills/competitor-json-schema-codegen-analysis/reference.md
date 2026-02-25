# Report template and keyword source

Copy-paste this structure when writing a competitor research report. Fill every section from the cloned repo and vendored specs only.

**Report purpose**: Research reports are standalone knowledge. Use them first to answer questions about a library; fall back to the cloned repo only when deeper implementation detail is needed.

**Diagrams**: Include Mermaid diagrams in architecture sections (e.g. High-level architecture, Medium-level architecture) where they clarify the narrative. Use valid Mermaid: node IDs with no spaces (camelCase, PascalCase, or underscores); double-quote labels that contain special characters; no HTML in labels; no explicit colors or styling.

---

## Report structure (template)

```markdown
# <Library name> — Research report

## Metadata

- **Library name**:
- **Repo URL**:
- **Clone path**: `research/repos/<lang>/<name>/`
- **Language**:
- **License**:

## Summary

[One paragraph: schema → codegen, target language(s).]

## JSON Schema support

[Which drafts/versions (e.g. draft-04, 2019-09, 2020-12); full vs subset.]

## Keyword support table

[One row per keyword. Columns: Keyword | Implemented (yes / no / partial) | Notes. Derive keyword list from vendored meta-schemas; see “Keyword list source” below.]

| Keyword | Implemented | Notes |
|---------|-------------|-------|
| ...     | ...         | ...   |

## Constraints

[Does it use validation keywords only for structure, or also enforce constraints (minLength, minItems, etc.) in generated code or at runtime?]

## High-level architecture

[Main components, pipeline: parse schema → intermediate → emit code. Include a Mermaid diagram (e.g. flowchart) of the pipeline where it helps.]

## Medium-level architecture

[Key modules/classes, data structures, how $ref is resolved. Include Mermaid diagram(s) where they clarify ref resolution or expansion flow.]

## Low-level details

[Only where necessary for important features; link to keyword table or medium-level section.]

## Output and integration

- **Vendored vs build-dir**: [Checked-in output vs gitignored build dir; configurable?]
- **API vs CLI**: [Library, CLI, or both; macros vs builder/codegen API.]
- **Writer model**: [File-only vs generic writer (e.g. Write, Vec<u8>, String).]

## Configuration

[Model/serialization settings, naming, map types, optional deps (e.g. uuid, chrono).]

## Pros/cons

[Technical decisions, trade-offs, strengths/weaknesses.]

## Testability

[How the project is tested (unit, integration, fixtures); how to run its tests; notes on running our generator against their test schemas later.]

## Performance

[Any built-in benchmarks, how they measure (wall time, instructions), where to find them. Note entry points useful for future benchmarking (e.g. CLI command, API call) so the library can be run against shared fixtures.]

## Determinism and idempotency

[Whether generated output is deterministic and idempotent. Note: Are models, fields, or other artifacts sorted (e.g. alphabetically) so that repeated invocations with the same input produce identical output? When input changes slightly, do diffs stay minimal (no large reshuffles or reordering)? Derive from code, tests, or docs in the cloned repo. Use "Unknown" or "Not applicable" if evidence is absent.]

## Enum handling

[How the library implements JSON Schema `enum`. Cover: (1) Duplicate entries — e.g. `["a", "a"]` — does the library dedupe, error, or emit duplicate variants? (2) Namespace/case collisions — e.g. `"a"` and `"A"` — does the library produce distinct variants so both are de/serializable and usable without losing either? Derive from code, tests, or docs. Use "Unknown" if evidence is absent.]

## Reverse generation (Schema from types)

[Whether the library can generate JSON Schema definitions from language structs/classes/POJOs (code → schema). If yes, describe the mechanism and scope. If no or unclear, state "No" or "Unknown".]

## Multi-language output

[Whether the library generates models only in its implementation language or can emit code for other languages (e.g. a Rust library generating TypeScript or Go). List supported output languages if multi-language. Use "Unknown" if evidence is absent.]

## Model deduplication and $ref/$defs

[When the same object shape appears in multiple distinct locations in the schema (e.g. identical inline object definitions in two different branches), does the library dedupe into a single generated type or emit separate copies? How does this interact with `$defs` and `$ref` (see [modular JSON Schema](https://json-schema.org/understanding-json-schema/structuring#modular-json-schema-combination))? Derive from code or tests. Use "Unknown" if evidence is absent.]

## Validation (schema + JSON → errors)

[Whether the library can validate a JSON payload against a JSON Schema. Inputs: schema definition and JSON entity. Output: report or list of validation errors. If yes, describe how (e.g. separate API, same binary). If no or only partial, state "No" or describe the gap. Use "Unknown" if evidence is absent.]
```

---

## Keyword list source

Build the keyword table from **vendored specs only**. For draft 2020-12, use the meta-schema files under:

- `specs/json-schema.org/draft/2020-12/meta/core.json`
- `specs/json-schema.org/draft/2020-12/meta/applicator.json`
- `specs/json-schema.org/draft/2020-12/meta/validation.json`
- `specs/json-schema.org/draft/2020-12/meta/meta-data.json`
- `specs/json-schema.org/draft/2020-12/meta/unevaluated.json`
- `specs/json-schema.org/draft/2020-12/meta/format-annotation.json`
- `specs/json-schema.org/draft/2020-12/meta/format-assertion.json`
- `specs/json-schema.org/draft/2020-12/meta/content.json`

Collect the union of all `properties` keys from these JSON files (and from `$defs` where they define schema keywords). For libraries that target older drafts (e.g. draft-04, 2019-09), add or use keywords from the corresponding meta-schemas under `specs/json-schema.org/` for that draft.

Do not invent keywords; every row in the table must correspond to a keyword from the vendored meta-schemas.
