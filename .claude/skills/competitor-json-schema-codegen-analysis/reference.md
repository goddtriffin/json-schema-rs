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
- **Analyzed version**: [Short commit hash and commit date of the clone's `HEAD` (`git -C <clone> log -1 --format='%h %cs'`), or the crate/package version for non-Git sources.]
- **Report date**: [Date the report was written or last refreshed.]

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

## Algorithms and data structures

[Mechanism-level explanation, with file/function anchors, of how the library actually works. Cover each subsection that applies; write "Not applicable" (with a reason) for those that do not. For each: the data structure(s) used, the algorithm/control flow, complexity or cost where evident, and the design trade-off.]

### Schema representation

[In-memory form of a parsed schema: typed struct vs raw JSON value vs arena/graph; how keywords, unknown keywords, and boolean schemas are stored; ownership/borrowing/lifetimes.]

### Reference resolution and registry

[How `$id`/`$ref`/`$anchor`/`$dynamicRef` (and legacy `id`/`$recursiveRef`) are resolved: registry/map structure, base-URI tracking, lazy vs eager resolution, remote fetching, and cycle detection/handling.]

### Compiled / intermediate form

[What the schema is lowered into before validation or emission (tree of validator objects, closures, arena + indices, bytecode/instruction list, generated source, type graph/IR). Describe the node types and how they link.]

### Core algorithm

[Validator: the evaluation loop — dispatch per keyword, short-circuiting, `is_valid` vs full-error paths, annotation tracking for `unevaluated*`. Codegen: type inference/selection, composition handling (`allOf` merge, `oneOf`/`anyOf` → unions/enums), naming and dedup, emission order. Reverse codegen: type → schema walk.]

### Error and output model

[Error/diagnostic data structure, instance/schema path tracking, output formats, how errors are collected and propagated.]

## Low-level details

[Notable implementation specifics not covered above: edge-case handling, numeric representation (big ints, decimals, float precision in `multipleOf`), string length counting (code points vs bytes vs UTF-16), recursion depth limits, untrusted-input hardening.]

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

[Analyze from code only — never run benchmarks or build the competitor; measured comparisons belong to our harness (`make benchmark`).]

### Performance techniques

[For each that applies, explain the technique, where it lives (file/function), and what it buys. Write "None found" when the code has none.]

- **Compile vs interpret**: [Is work moved ahead of time (schema compilation, code generation, specialization per keyword)? What is computed once vs per validation?]
- **Memory layout and allocation**: [Arena/flat vectors vs pointer trees, small-vector/inline storage, interning, borrowing vs cloning, avoiding per-instance allocations, lazy error/path construction.]
- **Caching and memoization**: [Compiled-schema caches, resolved-ref caches, regex caches, format-check caches; keys and invalidation.]
- **Fast paths and short-circuiting**: [Boolean-only validation path, early exit, keyword ordering, type-check-first, specialized validators for common shapes, discriminator shortcuts.]
- **Regex handling**: [Engine(s) used, ECMA-262 compatibility/translation, compile-once caching, protection against catastrophic backtracking (ReDoS).]
- **Concurrency**: [Thread safety of compiled schemas, parallel validation/generation, shared vs per-thread state.]

### Benchmarks in repo

[Built-in benchmarks (location, what they measure, fixtures), plus entry points useful for running the library against shared fixtures in our harness (e.g. CLI command, API call).]

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
