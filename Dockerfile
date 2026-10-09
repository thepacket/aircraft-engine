# syntax=docker/dockerfile:1
# Multi-stage build: compile the Rust core to WebAssembly, bundle the web IDE,
# then serve the static output with nginx. The running container holds no
# simulation code: everything executes in the visitor's browser.

# ---- Stage 1: Rust -> wasm --------------------------------------------------
FROM rust:1-slim-bookworm AS wasm
RUN rustup target add wasm32-unknown-unknown \
 && cargo install wasm-bindgen-cli --version 0.2.126 --locked
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN cargo build --release --target wasm32-unknown-unknown -p engine-wasm \
 && mkdir -p /out \
 && wasm-bindgen --target web --out-dir /out --out-name engine_wasm \
      target/wasm32-unknown-unknown/release/engine_wasm.wasm

# ---- Stage 2: web bundle ----------------------------------------------------
FROM node:20-alpine AS web
WORKDIR /web
COPY web/package.json web/package-lock.json ./
RUN npm ci
COPY web ./
# The IDE's Model tab renders docs/MODEL.md (imported as ../../docs/MODEL.md)
COPY docs /docs
COPY --from=wasm /out ./src/wasm
RUN npm run build

# ---- Stage 3: static server -------------------------------------------------
FROM nginx:1.27-alpine
COPY deploy/nginx.conf /etc/nginx/conf.d/default.conf
COPY --from=web /web/dist /usr/share/nginx/html
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=3s CMD wget -qO- http://127.0.0.1:8080/ >/dev/null || exit 1
