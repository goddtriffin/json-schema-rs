# syntax=docker/dockerfile:1
#
# Bowtie harness image for json-schema-rs.
#
# Build from the REPOSITORY ROOT (the build context must contain the whole Cargo workspace):
#
#   docker build -f json_schema_rs_differential/Dockerfile -t localhost/json-schema-rs:latest .
#
# Run it through Bowtie with `-i image:localhost/json-schema-rs`. The context is trimmed by
# json_schema_rs_differential/Dockerfile.dockerignore.
#
# Builder and runtime are both Alpine: the harness is a static musl binary, so the runtime image
# needs nothing beyond a base filesystem.

FROM rust:1.99-alpine AS builder
WORKDIR /build
COPY . .
# Cargo.lock is gitignored, so a local lockfile (when present in the context) pins the same
# dependency versions as the host build, and a fresh clone resolves them. Cache mounts keep the
# registry and target dir across rebuilds; the binary is copied out of the cached target dir in
# the same step.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/target \
    BOWTIE_HARNESS_RUSTC_VERSION="$(rustc --version | cut -d ' ' -f 2)" \
    cargo build --release -p json-schema-rs-differential --bin bowtie_harness \
    && cp target/release/bowtie_harness /usr/local/bin/bowtie_harness

FROM alpine:3.22
COPY --from=builder /usr/local/bin/bowtie_harness /usr/local/bin/bowtie_harness
CMD ["/usr/local/bin/bowtie_harness"]
