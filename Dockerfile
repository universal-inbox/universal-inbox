FROM lukemathwalker/cargo-chef:latest-rust-1.57.0 as chef
WORKDIR /app

FROM chef as planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json


FROM chef as dep-builder
RUN cargo install cargo-make
RUN cargo install trunk --version 0.14.0
RUN rustup target add wasm32-unknown-unknown
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook -p universal-inbox --release --recipe-path recipe.json
RUN cargo chef cook -p universal-inbox-api --release --recipe-path recipe.json
RUN cargo chef cook -p universal-inbox-web --release --recipe-path recipe.json --target wasm32-unknown-unknown

FROM dep-builder as release-builder
COPY . .
RUN cargo make build-release
RUN sed -i 's#http://localhost:8000/api#/api#' web/dist/snippets/universal-inbox-web-*/js/api.js
RUN sed -i 's#http://localhost:8000/api#/api#' web/dist/index.html

FROM debian:bullseye-slim AS runtime
WORKDIR /app
RUN mkdir /data
COPY --from=release-builder /app/target/release/universal-inbox-api universal-inbox
COPY --from=release-builder /app/api/config/default.toml config/default.toml
COPY --from=release-builder /app/web/dist/ .
ENV UNIVERSAL_INBOX_APPLICATION.API_PATH /api
ENV UNIVERSAL_INBOX_APPLICATION.STATIC_PATH /
CMD ["/app/universal-inbox", "serve"]
