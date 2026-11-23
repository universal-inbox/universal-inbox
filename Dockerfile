FROM ubuntu:24.04 as mise

SHELL ["/bin/bash", "-o", "pipefail", "-c"]
ENV MISE_YES=1
ENV MISE_LOCKED=1
ENV MISE_TRUSTED_CONFIG_PATHS=/app
ENV PATH="/root/.local/share/mise/shims:/root/.local/bin:${PATH}"
COPY dev/install-mise.sh /tmp/install-mise.sh
# hadolint ignore=DL3008
RUN apt-get update \
  && apt-get install -y --no-install-recommends build-essential pkg-config libssl-dev curl ca-certificates git \
  && rm -rf /var/lib/apt/lists/* \
  && bash /tmp/install-mise.sh \
  && rm /tmp/install-mise.sh

FROM mise as base
WORKDIR /app
COPY mise.toml mise.lock ./
COPY .common-rust.justfile .common-rust.justfile
COPY .justfile .justfile
COPY web/.justfile web/.justfile
COPY api/.justfile api/.justfile
COPY doc/.justfile doc/.justfile
COPY docker/.justfile docker/.justfile
RUN mise install rust node just sccache aqua:cargo-bins/cargo-binstall cargo:cargo-chef cargo:trunk cargo:wasm-bindgen-cli conda:binaryen conda:brotli-bin \
  && echo "Installed Packages."

FROM base as planner
COPY web/.cargo web/.cargo
COPY Cargo.toml Cargo.lock ./
COPY api/Cargo.toml api/Cargo.toml
COPY web/Cargo.toml web/Cargo.toml
RUN cargo chef prepare --recipe-path recipe.json

FROM rust:1.98.1-bookworm as tools
RUN cargo install sqlx-cli --version 0.9.0

FROM base as dep-web-builder
COPY .common-rust.justfile .common-rust.justfile
COPY web/.justfile web/.justfile
COPY web/index-empty.html web/index.html
COPY web/.cargo web/.cargo
# Create fake index.html for Trunk build to succeed without the real index.html
COPY web/package.json web/package.json
COPY web/package-lock.json web/package-lock.json
RUN just web install
COPY --from=planner /app/recipe.json recipe.json
# Only dependencies will be compiled as cargo chef has modfied main.rs and lib.rs to be empty
RUN cargo chef cook \
  --release \
  -p universal-inbox-web \
  --recipe-path recipe.json \
  --target wasm32-unknown-unknown \
  --no-build \
  && just web build-release

FROM base as dep-api-builder
COPY .common-rust.justfile .common-rust.justfile
COPY api/.justfile api/.justfile
COPY --from=planner /app/recipe.json recipe.json
# Only dependencies will be compiled as cargo chef has modfied main.rs and lib.rs to be empty
RUN cargo chef cook \
  --release \
  -p universal-inbox-api \
  --recipe-path recipe.json \
  --no-build \
  && just api build-release

FROM dep-web-builder as release-web-builder
ARG VERSION
ENV VERSION=${VERSION}
COPY Cargo.toml Cargo.lock ./
COPY web/Cargo.toml web/Cargo.toml
COPY src src
COPY web web
RUN just web build-assets \
  && just web build-release \
  && just web compress

FROM dep-api-builder as release-api-builder
ARG VERSION
ENV VERSION=${VERSION}
COPY Cargo.toml Cargo.lock ./
COPY api/Cargo.toml api/Cargo.toml
COPY src src
COPY api api
RUN just api build-release

FROM debian:trixie-slim as runtime
ARG VERSION
ENV VERSION=${VERSION}
WORKDIR /app
RUN mkdir /data
COPY docker/universal-inbox-entrypoint universal-inbox-entrypoint
COPY --from=release-api-builder /app/target/release/universal-inbox-api universal-inbox
# hadolint ignore=DL3008
RUN apt-get update \
  && apt-get install -y --no-install-recommends ca-certificates patchelf curl \
  && patchelf --set-interpreter /usr/bin/ld.so universal-inbox \
  && apt-get purge -y patchelf \
  && rm -rf /var/lib/apt/lists/*
COPY --from=release-api-builder /app/api/config/default.toml config/default.toml
COPY --from=release-api-builder /app/api/config/prod.toml config/prod.toml
COPY --from=release-api-builder /app/api/migrations migrations
COPY --from=tools /usr/local/cargo/bin/sqlx /usr/local/bin/sqlx
COPY --from=release-web-builder /app/web/public/ statics
ENV CONFIG_FILE /app/config/prod.toml
ENV UNIVERSAL_INBOX__APPLICATION__VERSION=${VERSION}
# Create non-root user owning /app (writable by the entrypoint, which rewrites
# /app/config/prod.toml at startup) and /data (intended writable volume mount).
RUN groupadd -r -g 10001 ui \
  && useradd -r -u 10001 -g ui -d /app -s /usr/sbin/nologin ui \
  && chown -R ui:ui /app /data
USER 10001:10001
ENTRYPOINT ["/app/universal-inbox-entrypoint"]
CMD ["serve"]
