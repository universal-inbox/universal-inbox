FROM lukemathwalker/cargo-chef:latest-rust-1.70.0-bookworm as chef
WORKDIR /app

FROM chef as planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef as dep-builder
RUN cargo install cargo-binstall --version 1.3.0
RUN cargo binstall -y cargo-make --version 0.36.8

FROM dep-builder as dep-web-builder
RUN cargo binstall -y trunk --version 0.16.0
RUN cargo binstall -y rtx-cli --version 1.32.0
RUN rustup target add wasm32-unknown-unknown
RUN rtx install nodejs@lts
RUN rtx global nodejs@lts
COPY --from=planner /app/web/package.json web/package.json
COPY --from=planner /app/web/package-lock.json web/package-lock.json
RUN cd web && rtx exec -- npm install --dev
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook -p universal-inbox-web --release --recipe-path recipe.json --target wasm32-unknown-unknown

FROM dep-builder as dep-api-builder
RUN cargo install sqlx-cli --version 0.6.2
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook -p universal-inbox-api --release --recipe-path recipe.json

FROM dep-web-builder as release-web-builder
COPY Makefile.toml Cargo.toml Cargo.lock ./
COPY src src
COPY web web
RUN rtx exec -- cargo make --cwd web build-release
RUN sed -i 's#http://localhost:8000/api#/api#' web/dist/snippets/universal-inbox-web-*/js/api.js
RUN sed -i 's#http://localhost:8000/api#/api#' web/dist/index.html

FROM dep-api-builder as release-api-builder
COPY Makefile.toml Cargo.toml Cargo.lock ./
COPY src src
COPY api api
RUN cargo make --cwd api build-release

FROM debian:bookworm-slim AS runtime
WORKDIR /app
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
RUN mkdir /data
COPY --from=release-api-builder /app/target/release/universal-inbox-api universal-inbox
COPY --from=release-api-builder /app/api/config/default.toml config/default.toml
COPY --from=release-api-builder /app/api/config/prod.toml config/prod.toml
COPY --from=release-api-builder /app/api/migrations migrations
COPY --from=release-api-builder /usr/local/cargo/bin/sqlx /usr/local/cargo/bin/sqlx
COPY --from=release-web-builder /app/web/dist/ statics
ENV CONFIG_FILE /app/config/prod.toml
ENTRYPOINT ["/app/universal-inbox"]
CMD ["serve"]
