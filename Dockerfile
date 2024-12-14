FROM jetpackio/devbox:latest as devbox

FROM devbox as base
WORKDIR /app
COPY --chown="${DEVBOX_USER}:${DEVBOX_USER}" devbox.json devbox.json
COPY --chown="${DEVBOX_USER}:${DEVBOX_USER}" devbox.lock devbox.lock
COPY --chown="${DEVBOX_USER}:${DEVBOX_USER}" justfile justfile
RUN sudo chown -R "${DEVBOX_USER}:${DEVBOX_USER}" /app
RUN devbox run -- echo "Installed Packages."

FROM base as planner
COPY --chown="${DEVBOX_USER}:${DEVBOX_USER}" web/.cargo web/.cargo
COPY --chown="${DEVBOX_USER}:${DEVBOX_USER}" Cargo.toml Cargo.lock ./
COPY --chown="${DEVBOX_USER}:${DEVBOX_USER}" api/Cargo.toml api/Cargo.toml
COPY --chown="${DEVBOX_USER}:${DEVBOX_USER}" web/Cargo.toml web/Cargo.toml
RUN devbox run -- cargo chef prepare --recipe-path recipe.json

FROM rust:1.82.0-bookworm as tools
RUN cargo install sqlx-cli --version 0.8.2

FROM base as dep-web-builder
COPY --chown="${DEVBOX_USER}:${DEVBOX_USER}" web/justfile web/justfile
COPY --chown="${DEVBOX_USER}:${DEVBOX_USER}" web/.cargo web/.cargo
# Create fake index.html for Trunk build to succeed without the real index.html
RUN touch web/index.html
COPY --chown="${DEVBOX_USER}:${DEVBOX_USER}" web/package.json web/package.json
COPY --chown="${DEVBOX_USER}:${DEVBOX_USER}" web/package-lock.json web/package-lock.json
RUN devbox run -- just web/install
COPY --chown="${DEVBOX_USER}:${DEVBOX_USER}" --from=planner /app/recipe.json recipe.json
RUN devbox run -- cargo chef cook --release -p universal-inbox-web --recipe-path recipe.json --target wasm32-unknown-unknown --no-build
# Only dependencies will be compiled as cargo chef has modfied main.rs and lib.rs to be empty
RUN devbox run -- just web/build-release

FROM base as dep-api-builder
COPY --chown="${DEVBOX_USER}:${DEVBOX_USER}" api/justfile api/justfile
COPY --chown="${DEVBOX_USER}:${DEVBOX_USER}" --from=planner /app/recipe.json recipe.json
RUN devbox run -- cargo chef cook --release -p universal-inbox-api --recipe-path recipe.json --no-build
# Only dependencies will be compiled as cargo chef has modfied main.rs and lib.rs to be empty
RUN devbox run -- just api/build-release

FROM dep-web-builder as release-web-builder
ARG VERSION
COPY --chown="${DEVBOX_USER}:${DEVBOX_USER}" Cargo.toml Cargo.lock ./
COPY --chown="${DEVBOX_USER}:${DEVBOX_USER}" web/Cargo.toml web/Cargo.toml
COPY --chown="${DEVBOX_USER}:${DEVBOX_USER}" src src
COPY --chown="${DEVBOX_USER}:${DEVBOX_USER}" web web
ENV VERSION=${VERSION}
RUN devbox run -- just web/build-release
RUN sed -i 's#http://localhost:8000/api#/api#' web/dist/snippets/universal-inbox-web-*/js/api.js
RUN sed -i 's#http://localhost:8000/api#/api#' web/dist/index.html

FROM dep-api-builder as release-api-builder
COPY --chown="${DEVBOX_USER}:${DEVBOX_USER}" Cargo.toml Cargo.lock ./
COPY --chown="${DEVBOX_USER}:${DEVBOX_USER}" api/Cargo.toml api/Cargo.toml
COPY --chown="${DEVBOX_USER}:${DEVBOX_USER}" src src
COPY --chown="${DEVBOX_USER}:${DEVBOX_USER}" api api
RUN devbox run -- just api/build-release

FROM debian:bookworm-slim AS runtime
ARG VERSION
WORKDIR /app
RUN mkdir /data
COPY docker/universal-inbox-entrypoint universal-inbox-entrypoint
COPY --from=release-api-builder /app/target/release/universal-inbox-api universal-inbox
RUN apt-get update \
  && apt-get install -y ca-certificates patchelf curl \
  && patchelf --set-interpreter /usr/bin/ld.so universal-inbox \
  && apt-get purge -y patchelf \
  && rm -rf /var/lib/apt/lists/*
COPY --from=release-api-builder /app/api/config/default.toml config/default.toml
COPY --from=release-api-builder /app/api/config/prod.toml config/prod.toml
COPY --from=release-api-builder /app/api/migrations migrations
COPY --from=tools /usr/local/cargo/bin/sqlx /usr/local/bin/sqlx
COPY --from=release-web-builder /app/web/dist/ statics
ENV CONFIG_FILE /app/config/prod.toml
ENV UNIVERSAL_INBOX__APPLICATION__VERSION=${VERSION}
ENTRYPOINT ["/app/universal-inbox-entrypoint"]
CMD ["serve"]
