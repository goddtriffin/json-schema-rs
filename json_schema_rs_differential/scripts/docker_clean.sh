#!/usr/bin/env bash
#
# docker_clean.sh — remove this repository's Docker containers, images and volumes (`make
# docker-clean`):
#   - everything labelled repo=json-schema-rs (our Bowtie harness image, and the containers
#     Bowtie starts from it, which inherit the label);
#   - the Bowtie competitor images under ghcr.io/bowtie-json-schema/, which only the
#     differential harness pulls, and any containers left from them.
# Nothing else is selected: no prune beyond these, no unlabelled deletes.

set -euo pipefail

LABEL="repo=json-schema-rs"
COMPETITOR_IMAGES="ghcr.io/bowtie-json-schema/*"

if ! docker info >/dev/null 2>&1; then
  echo "error: no running container engine. Start Docker." >&2
  exit 1
fi

# lines <command...>: run the command and print its non-empty output lines, without the \r
# Docker on Windows may add.
lines() {
  local line
  while IFS= read -r line; do
    line="${line%$'\r'}"
    [[ -n "${line}" ]] && printf '%s\n' "${line}"
  done < <("$@")
}

# Images as `<id> <repository>:<tag>`: ours (by label) and the competitors' (by name).
images="$(
  lines docker images --filter "label=${LABEL}" --format '{{.ID}} {{.Repository}}:{{.Tag}}'
  lines docker images --filter "reference=${COMPETITOR_IMAGES}" --format '{{.ID}} {{.Repository}}:{{.Tag}}'
)"

# Containers: labelled ones, plus any started from one of the images above.
containers="$(
  {
    lines docker ps -aq --filter "label=${LABEL}"
    while read -r id _; do
      [[ -n "${id}" ]] && lines docker ps -aq --filter "ancestor=${id}"
    done <<<"${images}"
  } | sort -u
)"

if [[ -n "${containers}" ]]; then
  echo "==> removing containers"
  # shellcheck disable=SC2086
  docker rm -f ${containers}
fi

if [[ -n "${images}" ]]; then
  echo "==> removing images"
  # By name, so an image that also carries another repository's tag keeps that tag; untagged
  # (dangling) images by id.
  while read -r id reference; do
    [[ -z "${id}" ]] && continue
    case "${reference}" in
      *"<none>"*) docker image rm "${id}" ;;
      *) docker image rm "${reference}" ;;
    esac
  done <<<"$(printf '%s\n' "${images}" | sort -u)"
fi

volumes="$(lines docker volume ls -q --filter "label=${LABEL}")"
if [[ -n "${volumes}" ]]; then
  echo "==> removing volumes"
  # shellcheck disable=SC2086
  docker volume rm ${volumes}
fi

echo "==> done: no json-schema-rs containers, images or volumes left"
