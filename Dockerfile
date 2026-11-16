# syntax=docker/dockerfile:1

FROM node:22-alpine AS admin-ui
WORKDIR /app
COPY web/package.json web/package-lock.json ./
RUN npm ci
COPY web/ ./
RUN npm run build:admin

FROM rust:1.92-bookworm AS builder
RUN apt-get update && apt-get install -y pkg-config libssl-dev clang mold \
 && rm -rf /var/lib/apt/lists/*
ENV RUSTFLAGS="-C linker=clang -C link-arg=-fuse-ld=mold"
WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY migrations ./migrations
COPY --from=admin-ui /app/sites/admin/dist ./web/sites/admin/dist

ARG BINS="coral-api coral-bot coral-admin coral-sync coral-verify"
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    cargo build --release $(printf -- '--bin %s ' $BINS) \
 && mkdir -p /out/clients \
 && for bin in $BINS; do cp "target/release/$bin" /out/; done \
 && cargo build --release --manifest-path crates/clients/Cargo.toml --bins --message-format=json-render-diagnostics \
  | grep -o '"executable":"[^"]*"' | cut -d '"' -f 4 | xargs -r cp -t /out/clients/


FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y ca-certificates libssl3 curl \
 && rm -rf /var/lib/apt/lists/*
ENV RUST_LOG=info

FROM runtime AS renderer
RUN apt-get update && apt-get install -y mesa-vulkan-drivers \
 && rm -rf /var/lib/apt/lists/*


FROM renderer AS coral-api
COPY --from=builder /out/coral-api /out/clients/ /usr/local/bin/
EXPOSE 8000
CMD ["coral-api"]

FROM renderer AS coral-bot
COPY --from=builder /out/coral-bot /usr/local/bin/
CMD ["coral-bot"]

FROM runtime AS coral-admin
COPY --from=builder /out/coral-admin /usr/local/bin/
EXPOSE 8080
CMD ["coral-admin"]

FROM runtime AS coral-sync
COPY --from=builder /out/coral-sync /usr/local/bin/
CMD ["coral-sync"]

FROM runtime AS coral-verify
COPY --from=builder /out/coral-verify /usr/local/bin/
EXPOSE 25565
CMD ["coral-verify"]
