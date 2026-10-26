# syntax=docker/dockerfile:1

FROM node:22-alpine AS admin-ui
WORKDIR /app
COPY crates/coral-admin/ui/package.json crates/coral-admin/ui/package-lock.json ./
RUN npm ci
COPY crates/coral-admin/ui/ ./
RUN npm run build

FROM rust:1.92-bookworm AS builder
RUN apt-get update && apt-get install -y pkg-config libssl-dev clang mold \
 && rm -rf /var/lib/apt/lists/*
ENV RUSTFLAGS="-C linker=clang -C link-arg=-fuse-ld=mold"
WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY migrations ./migrations
COPY --from=admin-ui /app/dist ./crates/coral-admin/ui/dist

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


FROM node:22-alpine AS web-deps
WORKDIR /app
COPY coral-web/package.json coral-web/package-lock.json ./
RUN npm ci

FROM node:22-alpine AS web-builder
WORKDIR /app
COPY --from=web-deps /app/node_modules ./node_modules
COPY coral-web/ ./
RUN npm run build

FROM node:22-alpine AS coral-web
WORKDIR /app
ENV NODE_ENV=production
COPY --from=web-builder /app/.next/standalone ./
COPY --from=web-builder /app/.next/static ./.next/static
COPY --from=web-builder /app/public ./public
EXPOSE 3000
CMD ["node", "server.js"]
