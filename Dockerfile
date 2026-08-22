FROM rust:1-bookworm AS build
WORKDIR /app
COPY Cargo.toml Cargo.lock AGENT.md ./
COPY src ./src
COPY static ./static
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --uid 1000 --create-home gist
COPY --from=build /app/target/release/gist /usr/local/bin/gist
ENV GIST_BIND=0.0.0.0:8787 \
    GIST_DATA=/data
VOLUME /data
EXPOSE 8787
USER gist
WORKDIR /data
ENTRYPOINT ["gist"]
