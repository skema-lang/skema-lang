# Runtime-only image. The binaries are pre-built by the CI "build" stage
# (`cargo build --release`) and copied in as build context — this Dockerfile
# only assembles the final image, it does not compile Rust. Recompiling here
# via kaniko would just duplicate work already done upstream in the pipeline.
#
# The base image's glibc must match what the binaries were linked against:
# CI compiles with `rust:1.85-bookworm`, so the runtime stage below is
# `debian:bookworm-slim`. If one changes, the other must too.
FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY target/release/skema /usr/local/bin/skema
COPY target/release/skema-lsp /usr/local/bin/skema-lsp

ENTRYPOINT ["skema"]
CMD ["--help"]
