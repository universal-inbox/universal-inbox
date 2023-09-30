FROM jetpackio/devbox:latest as devbox

FROM devbox as base
WORKDIR /app
COPY --chown="${DEVBOX_USER}:${DEVBOX_USER}" devbox.json devbox.json
COPY --chown="${DEVBOX_USER}:${DEVBOX_USER}" devbox.lock devbox.lock
RUN sudo chown -R "${DEVBOX_USER}:${DEVBOX_USER}" /app
RUN devbox run -- echo "Installed Packages."

FROM base as planner
COPY --chown="${DEVBOX_USER}:${DEVBOX_USER}" . .
RUN devbox run -- cargo chef prepare --recipe-path recipe.json

FROM rust:1.70.0-bookworm as tools
RUN cargo install sqlx-cli --version 0.6.2

FROM base as dep-web-builder
COPY --chown="${DEVBOX_USER}:${DEVBOX_USER}" web/justfile web/justfile
COPY --chown="${DEVBOX_USER}:${DEVBOX_USER}" web/package.json web/package.json
COPY --chown="${DEVBOX_USER}:${DEVBOX_USER}" web/package-lock.json web/package-lock.json
RUN devbox run -- just web/install
COPY --chown="${DEVBOX_USER}:${DEVBOX_USER}" --from=planner /app/recipe.json recipe.json
RUN devbox run -- cargo chef cook --release -p universal-inbox-web --recipe-path recipe.json --target wasm32-unknown-unknown

FROM base as dep-api-builder
COPY --chown="${DEVBOX_USER}:${DEVBOX_USER}" api/justfile api/justfile
COPY --chown="${DEVBOX_USER}:${DEVBOX_USER}" --from=planner /app/recipe.json recipe.json
RUN devbox run -- cargo chef cook --release -p universal-inbox-api --recipe-path recipe.json

FROM dep-web-builder as release-web-builder
COPY --chown="${DEVBOX_USER}:${DEVBOX_USER}" Cargo.toml Cargo.lock ./
COPY --chown="${DEVBOX_USER}:${DEVBOX_USER}" src src
COPY --chown="${DEVBOX_USER}:${DEVBOX_USER}" web web
RUN devbox run -- just web/build-release
RUN sed -i 's#http://localhost:8000/api#/api#' web/dist/snippets/universal-inbox-web-*/js/api.js
RUN sed -i 's#http://localhost:8000/api#/api#' web/dist/index.html

FROM dep-api-builder as release-api-builder
COPY --chown="${DEVBOX_USER}:${DEVBOX_USER}" Cargo.toml Cargo.lock ./
COPY --chown="${DEVBOX_USER}:${DEVBOX_USER}" src src
COPY --chown="${DEVBOX_USER}:${DEVBOX_USER}" api api
RUN devbox run -- just api/build-release

FROM debian:bookworm-slim AS runtime
WORKDIR /app
RUN mkdir /data
COPY --from=release-api-builder /app/target/release/universal-inbox-api universal-inbox
RUN apt-get update \
    && apt-get install -y ca-certificates patchelf \
    && patchelf --set-interpreter /usr/bin/ld.so universal-inbox \
    && apt-get purge -y patchelf \
    && rm -rf /var/lib/apt/lists/*
COPY --from=release-api-builder /app/api/config/default.toml config/default.toml
COPY --from=release-api-builder /app/api/config/prod.toml config/prod.toml
COPY --from=release-api-builder /app/api/migrations migrations
COPY --from=tools /usr/local/cargo/bin/sqlx /usr/local/bin/sqlx
COPY --from=release-web-builder /app/web/dist/ statics
ENV CONFIG_FILE /app/config/prod.toml
ENTRYPOINT ["/app/universal-inbox"]
CMD ["serve"]
