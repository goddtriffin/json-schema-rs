#!/usr/bin/env bash
# Vendor JSON Schema specifications from json-schema.org and IETF.
# Run from repo root: ./specs/download.sh  or  make vendor-specs
# All URLs and destination paths are hard-coded below for easy refresh and updates.

set -e

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

fetch() {
  local url="$1"
  local path="$2"
  mkdir -p "$(dirname "$path")"
  echo "Fetching $path ..."
  curl -L -f -s -o "$path" "$url" || { echo "Failed: $url" >&2; exit 1; }
}

# ---- IETF (Relative JSON Pointer, JSON Reference) ----
BASE_IETF="https://tools.ietf.org/html"
fetch "${BASE_IETF}/draft-bhutton-relative-json-pointer-00" "specs/ietf/draft-bhutton-relative-json-pointer-00.html"
fetch "${BASE_IETF}/draft-handrews-relative-json-pointer-02" "specs/ietf/draft-handrews-relative-json-pointer-02.html"
fetch "${BASE_IETF}/draft-handrews-relative-json-pointer-01" "specs/ietf/draft-handrews-relative-json-pointer-01.html"
fetch "${BASE_IETF}/draft-handrews-relative-json-pointer-00" "specs/ietf/draft-handrews-relative-json-pointer-00.html"
fetch "${BASE_IETF}/draft-pbryan-zyp-json-ref-03" "specs/ietf/draft-pbryan-zyp-json-ref-03.html"

# ---- Draft 0 ----
B="https://json-schema.org/draft-00"
fetch "${B}/draft-zyp-json-schema-00.txt" "specs/json-schema.org/draft-00/draft-zyp-json-schema-00.txt"
fetch "${B}/schema" "specs/json-schema.org/draft-00/schema.json"
fetch "${B}/hyper-schema" "specs/json-schema.org/draft-00/hyper-schema.json"

# ---- Draft 1 ----
B="https://json-schema.org/draft-01"
fetch "${B}/draft-zyp-json-schema-01.html" "specs/json-schema.org/draft-01/draft-zyp-json-schema-01.html"
fetch "${B}/schema" "specs/json-schema.org/draft-01/schema.json"
fetch "${B}/hyper-schema" "specs/json-schema.org/draft-01/hyper-schema.json"

# ---- Draft 2 ----
B="https://json-schema.org/draft-02"
fetch "${B}/draft-zyp-json-schema-02.txt" "specs/json-schema.org/draft-02/draft-zyp-json-schema-02.txt"
fetch "${B}/schema" "specs/json-schema.org/draft-02/schema.json"
fetch "${B}/hyper-schema" "specs/json-schema.org/draft-02/hyper-schema.json"

# ---- Draft 3 ----
B="https://json-schema.org/draft-03"
fetch "${B}/draft-zyp-json-schema-03.pdf" "specs/json-schema.org/draft-03/draft-zyp-json-schema-03.pdf"
fetch "${B}/schema" "specs/json-schema.org/draft-03/schema.json"
fetch "${B}/hyper-schema" "specs/json-schema.org/draft-03/hyper-schema.json"

# ---- Draft 4 ----
B="https://json-schema.org/draft-04"
fetch "${B}/draft-zyp-json-schema-04.html" "specs/json-schema.org/draft-04/draft-zyp-json-schema-04.html"
fetch "${B}/draft-fge-json-schema-validation-00.html" "specs/json-schema.org/draft-04/draft-fge-json-schema-validation-00.html"
fetch "${B}/draft-luff-json-hyper-schema-00.html" "specs/json-schema.org/draft-04/draft-luff-json-hyper-schema-00.html"
fetch "${B}/schema" "specs/json-schema.org/draft-04/schema.json"
fetch "${B}/hyper-schema" "specs/json-schema.org/draft-04/hyper-schema.json"

# ---- Draft 5 ----
B="https://json-schema.org/draft-05"
fetch "${B}/draft-wright-json-schema-00.pdf" "specs/json-schema.org/draft-05/draft-wright-json-schema-00.pdf"
fetch "${B}/draft-wright-json-schema-validation-00.pdf" "specs/json-schema.org/draft-05/draft-wright-json-schema-validation-00.pdf"
fetch "${B}/draft-wright-json-schema-hyperschema-00.pdf" "specs/json-schema.org/draft-05/draft-wright-json-schema-hyperschema-00.pdf"

# ---- Draft 6 ----
B="https://json-schema.org/draft-06"
fetch "${B}/draft-wright-json-schema-01.html" "specs/json-schema.org/draft-06/draft-wright-json-schema-01.html"
fetch "${B}/draft-wright-json-schema-validation-01.html" "specs/json-schema.org/draft-06/draft-wright-json-schema-validation-01.html"
fetch "${B}/draft-wright-json-schema-hyperschema-01.html" "specs/json-schema.org/draft-06/draft-wright-json-schema-hyperschema-01.html"
fetch "${B}/schema" "specs/json-schema.org/draft-06/schema.json"
fetch "${B}/hyper-schema" "specs/json-schema.org/draft-06/hyper-schema.json"

# ---- Draft 7 (current + obsolete) ----
B="https://json-schema.org/draft-07"
fetch "${B}/draft-handrews-json-schema-01.html" "specs/json-schema.org/draft-07/draft-handrews-json-schema-01.html"
fetch "${B}/draft-handrews-json-schema-00.pdf" "specs/json-schema.org/draft-07/draft-handrews-json-schema-00.pdf"
fetch "${B}/draft-handrews-json-schema-validation-01.html" "specs/json-schema.org/draft-07/draft-handrews-json-schema-validation-01.html"
fetch "${B}/draft-handrews-json-schema-validation-00.pdf" "specs/json-schema.org/draft-07/draft-handrews-json-schema-validation-00.pdf"
fetch "${B}/draft-handrews-json-schema-hyperschema-01.html" "specs/json-schema.org/draft-07/draft-handrews-json-schema-hyperschema-01.html"
fetch "${B}/draft-handrews-json-schema-hyperschema-00.html" "specs/json-schema.org/draft-07/draft-handrews-json-schema-hyperschema-00.html"
fetch "${B}/schema" "specs/json-schema.org/draft-07/schema.json"
fetch "${B}/hyper-schema" "specs/json-schema.org/draft-07/hyper-schema.json"
fetch "${B}/links" "specs/json-schema.org/draft-07/links.json"
fetch "${B}/hyper-schema-output" "specs/json-schema.org/draft-07/hyper-schema-output.json"

# ---- Draft 2019-09 ----
B="https://json-schema.org/draft/2019-09"
fetch "${B}/draft-handrews-json-schema-02.html" "specs/json-schema.org/draft/2019-09/draft-handrews-json-schema-02.html"
fetch "${B}/draft-handrews-json-schema-validation-02.html" "specs/json-schema.org/draft/2019-09/draft-handrews-json-schema-validation-02.html"
fetch "${B}/draft-handrews-json-schema-hyperschema-02.html" "specs/json-schema.org/draft/2019-09/draft-handrews-json-schema-hyperschema-02.html"
fetch "${B}/schema" "specs/json-schema.org/draft/2019-09/schema.json"
fetch "${B}/hyper-schema" "specs/json-schema.org/draft/2019-09/hyper-schema.json"
fetch "${B}/links" "specs/json-schema.org/draft/2019-09/links.json"
fetch "${B}/meta/hyper-schema" "specs/json-schema.org/draft/2019-09/meta/hyper-schema.json"
fetch "${B}/meta/meta-data" "specs/json-schema.org/draft/2019-09/meta/meta-data.json"
fetch "${B}/meta/content" "specs/json-schema.org/draft/2019-09/meta/content.json"
fetch "${B}/meta/format" "specs/json-schema.org/draft/2019-09/meta/format.json"
fetch "${B}/meta/validation" "specs/json-schema.org/draft/2019-09/meta/validation.json"
fetch "${B}/meta/applicator" "specs/json-schema.org/draft/2019-09/meta/applicator.json"
fetch "${B}/meta/core" "specs/json-schema.org/draft/2019-09/meta/core.json"
fetch "${B}/output/hyper-schema" "specs/json-schema.org/draft/2019-09/output/hyper-schema.json"
fetch "${B}/output/schema" "specs/json-schema.org/draft/2019-09/output/schema.json"
fetch "${B}/output/verbose-example" "specs/json-schema.org/draft/2019-09/output/verbose-example.json"

# ---- Draft 2020-12 (current + obsolete) ----
B="https://json-schema.org/draft/2020-12"
fetch "${B}/draft-bhutton-json-schema-01.html" "specs/json-schema.org/draft/2020-12/draft-bhutton-json-schema-01.html"
fetch "${B}/draft-bhutton-json-schema-00.html" "specs/json-schema.org/draft/2020-12/draft-bhutton-json-schema-00.html"
fetch "${B}/draft-bhutton-json-schema-validation-01.html" "specs/json-schema.org/draft/2020-12/draft-bhutton-json-schema-validation-01.html"
fetch "${B}/draft-bhutton-json-schema-validation-00.html" "specs/json-schema.org/draft/2020-12/draft-bhutton-json-schema-validation-00.html"
fetch "${B}/schema" "specs/json-schema.org/draft/2020-12/schema.json"
fetch "${B}/hyper-schema" "specs/json-schema.org/draft/2020-12/hyper-schema.json"
fetch "${B}/links" "specs/json-schema.org/draft/2020-12/links.json"
fetch "${B}/meta/meta-data" "specs/json-schema.org/draft/2020-12/meta/meta-data.json"
fetch "${B}/meta/content" "specs/json-schema.org/draft/2020-12/meta/content.json"
fetch "${B}/meta/format-assertion" "specs/json-schema.org/draft/2020-12/meta/format-assertion.json"
fetch "${B}/meta/format-annotation" "specs/json-schema.org/draft/2020-12/meta/format-annotation.json"
fetch "${B}/meta/unevaluated" "specs/json-schema.org/draft/2020-12/meta/unevaluated.json"
fetch "${B}/meta/validation" "specs/json-schema.org/draft/2020-12/meta/validation.json"
fetch "${B}/meta/applicator" "specs/json-schema.org/draft/2020-12/meta/applicator.json"
fetch "${B}/meta/core" "specs/json-schema.org/draft/2020-12/meta/core.json"
fetch "${B}/output/schema" "specs/json-schema.org/draft/2020-12/output/schema.json"
fetch "${B}/output/verbose-example" "specs/json-schema.org/draft/2020-12/output/verbose-example.json"

# ---- Release notes / migration ----
fetch "https://json-schema.org/draft-06/json-schema-release-notes" "specs/json-schema.org/release-notes/draft-06-json-schema-release-notes.html"
fetch "https://json-schema.org/draft-06/json-hyper-schema-release-notes" "specs/json-schema.org/release-notes/draft-06-json-hyper-schema-release-notes.html"
fetch "https://json-schema.org/draft-07/json-schema-release-notes" "specs/json-schema.org/release-notes/draft-07-json-schema-release-notes.html"
fetch "https://json-schema.org/draft-07/json-hyper-schema-release-notes" "specs/json-schema.org/release-notes/draft-07-json-hyper-schema-release-notes.html"
fetch "https://json-schema.org/draft/2019-09/release-notes" "specs/json-schema.org/release-notes/draft-2019-09-release-notes.html"
fetch "https://json-schema.org/draft/2020-12/release-notes" "specs/json-schema.org/release-notes/draft-2020-12-release-notes.html"

echo "Done. Vendored specs are under specs/."
