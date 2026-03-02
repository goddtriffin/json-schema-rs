# JSON Schema specifications (local copy)

This directory holds a **local copy** of every published JSON Schema
specification from [json-schema.org](https://json-schema.org/specification) and
related IETF drafts (Relative JSON Pointer, JSON Reference). The spec files
are **not** committed to the repo—they are gitignored. Contributors run
`make vendor_specs` or `./specs/download.sh` from the repository root to
download them; the result is a local, offline-capable copy under `specs/`.

## Contents

- **`json-schema.org/`** — Specification documents (HTML, PDF, TXT),
  meta-schemas (JSON), output schemas/examples, and release notes for drafts 00
  through 07, 2019-09, and 2020-12.
- **`ietf/`** — RFC 6901 (JSON Pointer, TXT) and IETF Internet-Drafts: Relative
  JSON Pointer and JSON Reference (HTML). JSON Pointer is used by JSON Schema for
  fragment identification; the `JsonPointer` type in code follows RFC 6901 for
  encoding and decoding.
- **`download.sh`** — Script that downloads all of the above. URLs and
  destination paths are hard-coded in the script.

## Updating (refreshing) the local spec copy

From the **repository root** run:

```bash
make vendor_specs
```

or:

```bash
./specs/download.sh
```

This re-downloads every file from the canonical URLs. The downloaded files are
gitignored and are not committed. If json-schema.org or IETF change URLs in the
future, edit the URLs in `specs/download.sh` and run the script again.

## Adding new specs

When new drafts or documents are published, add the URL and destination path to
`download.sh` (in the appropriate section).

## Source and licensing

- **json-schema.org**: Specifications and meta-schemas are from the
  [JSON Schema project](https://json-schema.org/). See the
  [JSON Schema repository](https://github.com/json-schema-org/json-schema-spec)
  and the project site for license and attribution.
- **IETF**: Relative JSON Pointer and JSON Reference are IETF Internet-Drafts.
  See [tools.ietf.org](https://tools.ietf.org/) and the IETF Trust for license
  and attribution.

We do not claim ownership of these materials; they are vendored for convenience
and reference.
