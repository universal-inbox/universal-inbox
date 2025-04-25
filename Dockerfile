FROM jetpackio/devbox:0.14.0 as devbox

FROM devbox as base
ENV PATH="/home/devbox/.cargo/bin:${PATH}"
ENV DOCKER_BUILD=true
WORKDIR /app
COPY --chown="${DEVBOX_USER}:${DEVBOX_USER}" devbox.json devbox.json
COPY --chown="${DEVBOX_USER}:${DEVBOX_USER}" devbox.lock devbox.lock
COPY --chown="${DEVBOX_USER}:${DEVBOX_USER}" justfile justfile
RUN chown -R "${DEVBOX_USER}:${DEVBOX_USER}" /app \
  && devbox run -- echo "Installed Packages."

FROM base as planner
COPY --chown="${DEVBOX_USER}:${DEVBOX_USER}" web/.cargo web/.cargo
COPY --chown="${DEVBOX_USER}:${DEVBOX_USER}" Cargo.toml Cargo.lock ./
COPY --chown="${DEVBOX_USER}:${DEVBOX_USER}" api/Cargo.toml api/Cargo.toml
COPY --chown="${DEVBOX_USER}:${DEVBOX_USER}" web/Cargo.toml web/Cargo.toml
RUN devbox run -- cargo chef prepare --recipe-path recipe.json

FROM rust:1.85.0-bookworm as tools
RUN cargo install sqlx-cli --version 0.8.3

FROM base as dep-web-builder
COPY --chown="${DEVBOX_USER}:${DEVBOX_USER}" web/justfile web/justfile
COPY --chown="${DEVBOX_USER}:${DEVBOX_USER}" web/index-empty.html web/index.html
COPY --chown="${DEVBOX_USER}:${DEVBOX_USER}" web/.cargo web/.cargo
# Create fake index.html for Trunk build to succeed without the real index.html
COPY --chown="${DEVBOX_USER}:${DEVBOX_USER}" web/package.json web/package.json
COPY --chown="${DEVBOX_USER}:${DEVBOX_USER}" web/package-lock.json web/package-lock.json
COPY --chown="${DEVBOX_USER}:${DEVBOX_USER}" web/scripts/npm-post-install.sh web/scripts/npm-post-install.sh
RUN devbox run -- just web/install
COPY --chown="${DEVBOX_USER}:${DEVBOX_USER}" --from=planner /app/recipe.json recipe.json
# Only dependencies will be compiled as cargo chef has modfied main.rs and lib.rs to be empty
RUN devbox run -- \
    cargo chef cook \
    --release \
    -p universal-inbox-web \
    --recipe-path recipe.json \
    --target wasm32-unknown-unknown \
    --no-build \
  && devbox run -- just web/build-release

FROM base as dep-api-builder
COPY --chown="${DEVBOX_USER}:${DEVBOX_USER}" api/justfile api/justfile
COPY --chown="${DEVBOX_USER}:${DEVBOX_USER}" --from=planner /app/recipe.json recipe.json
# Only dependencies will be compiled as cargo chef has modfied main.rs and lib.rs to be empty
RUN devbox run -- \
    cargo chef cook \
    --release \
    -p universal-inbox-api \
    --recipe-path recipe.json \
    --no-build \
  && devbox run -- just api/build-release

FROM dep-web-builder as release-web-builder
ARG VERSION
ENV VERSION=${VERSION}
COPY --chown="${DEVBOX_USER}:${DEVBOX_USER}" Cargo.toml Cargo.lock ./
COPY --chown="${DEVBOX_USER}:${DEVBOX_USER}" web/Cargo.toml web/Cargo.toml
COPY --chown="${DEVBOX_USER}:${DEVBOX_USER}" src src
COPY --chown="${DEVBOX_USER}:${DEVBOX_USER}" web web
RUN devbox run -- just web/build-assets \
  && devbox run -- just web/build-release \
  && sed -i 's#http://localhost:8000/api#/api#' web/public/index.html

FROM dep-api-builder as release-api-builder
ARG VERSION
ENV VERSION=${VERSION}
COPY --chown="${DEVBOX_USER}:${DEVBOX_USER}" Cargo.toml Cargo.lock ./
COPY --chown="${DEVBOX_USER}:${DEVBOX_USER}" api/Cargo.toml api/Cargo.toml
COPY --chown="${DEVBOX_USER}:${DEVBOX_USER}" src src
COPY --chown="${DEVBOX_USER}:${DEVBOX_USER}" api api
RUN devbox run -- just api/build-release

FROM debian:testing-slim AS runtime
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
ENTRYPOINT ["/app/universal-inbox-entrypoint"]
CMD ["serve"]
