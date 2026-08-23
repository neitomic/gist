# syntax=docker/dockerfile:1
# Native or cross (linux/amd64, linux/arm64) via buildx:
#   docker buildx build --platform linux/amd64,linux/arm64 -t gist .

FROM --platform=$BUILDPLATFORM rust:1-bookworm AS build
ARG TARGETPLATFORM
ARG TARGETARCH
WORKDIR /app

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        gcc-aarch64-linux-gnu \
        gcc-x86-64-linux-gnu \
        libc6-dev-arm64-cross \
        libc6-dev-amd64-cross \
        pkg-config \
    && rm -rf /var/lib/apt/lists/* \
    && case "$TARGETPLATFORM" in \
         linux/amd64) rustup target add x86_64-unknown-linux-gnu ;; \
         linux/arm64) rustup target add aarch64-unknown-linux-gnu ;; \
         *) echo "unsupported platform: $TARGETPLATFORM" >&2; exit 1 ;; \
       esac

COPY Cargo.toml Cargo.lock AGENT.md ./
COPY src ./src
COPY static ./static

ENV CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=x86_64-linux-gnu-gcc \
    CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc \
    CC_x86_64_unknown_linux_gnu=x86_64-linux-gnu-gcc \
    CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc \
    CARGO_TERM_COLOR=always

RUN --mount=type=cache,target=/usr/local/cargo/registry,id=gist-cargo-registry-${TARGETARCH} \
    --mount=type=cache,target=/app/target,id=gist-cargo-target-${TARGETARCH} \
    set -eux; \
    case "$TARGETPLATFORM" in \
      linux/amd64) rust_target=x86_64-unknown-linux-gnu ;; \
      linux/arm64) rust_target=aarch64-unknown-linux-gnu ;; \
      *) echo "unsupported platform: $TARGETPLATFORM" >&2; exit 1 ;; \
    esac; \
    cargo build --release --locked --target "$rust_target"; \
    install -Dm755 "target/${rust_target}/release/gist" /out/gist

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --uid 1000 --create-home gist
COPY --from=build /out/gist /usr/local/bin/gist
ENV GIST_BIND=0.0.0.0:8787 \
    GIST_DATA=/data
VOLUME /data
EXPOSE 8787
USER gist
WORKDIR /data
ENTRYPOINT ["gist"]
