# syntax=docker/dockerfile:1

FROM rust:1.92-bookworm AS chef
RUN cargo install cargo-chef --locked
WORKDIR /app

FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN cargo chef prepare --recipe-path recipe.json

FROM node:22-alpine AS admin-ui-deps
WORKDIR /app
COPY crates/coral-admin/ui/package.json crates/coral-admin/ui/package-lock.json ./
RUN npm ci

FROM node:22-alpine AS admin-ui-builder
WORKDIR /app
COPY --from=admin-ui-deps /app/node_modules ./node_modules
COPY crates/coral-admin/ui/ ./
RUN npm run build

FROM chef AS builder
RUN apt-get update && apt-get install -y \
    pkg-config libssl-dev clang mold \
 && rm -rf /var/lib/apt/lists/*

ENV RUSTFLAGS="-C linker=clang -C link-arg=-fuse-ld=mold"

COPY --from=planner /app/recipe.json recipe.json

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo chef cook --release --recipe-path recipe.json

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY migrations ./migrations
COPY --from=admin-ui-builder /app/dist ./crates/coral-admin/ui/dist

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release --bin coral-api --bin coral-bot --bin coral-admin --bin coral-sync --bin coral-verify && \
    cp target/release/coral-api target/release/coral-bot target/release/coral-admin target/release/coral-sync target/release/coral-verify /usr/local/bin/


FROM debian:bookworm-slim AS coral-api
RUN apt-get update && apt-get install -y \
    ca-certificates libssl3 curl mesa-vulkan-drivers \
 && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/local/bin/coral-api /usr/local/bin/
ENV RUST_LOG=info
EXPOSE 8000
CMD ["coral-api"]


FROM debian:bookworm-slim AS coral-bot
RUN apt-get update && apt-get install -y \
    ca-certificates libssl3 mesa-vulkan-drivers \
 && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/local/bin/coral-bot /usr/local/bin/
ENV RUST_LOG=info
CMD ["coral-bot"]


FROM debian:bookworm-slim AS coral-admin
RUN apt-get update && apt-get install -y \
    ca-certificates libssl3 \
 && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/local/bin/coral-admin /usr/local/bin/
ENV RUST_LOG=info
EXPOSE 8080
CMD ["coral-admin"]


FROM debian:bookworm-slim AS coral-sync
RUN apt-get update && apt-get install -y \
    ca-certificates libssl3 \
 && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/local/bin/coral-sync /usr/local/bin/
ENV RUST_LOG=info
EXPOSE 25565
CMD ["coral-sync"]


FROM debian:bookworm-slim AS coral-verify
RUN apt-get update && apt-get install -y \
    ca-certificates libssl3 \
 && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/local/bin/coral-verify /usr/local/bin/
ENV RUST_LOG=info
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
