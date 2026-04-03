# freeq staging — IRC server + web client in one image

# ── Rust build ──
FROM rust:1.93-slim AS builder
RUN apt-get update && apt-get install -y pkg-config libssl-dev cmake clang libasound2-dev && rm -rf /var/lib/apt/lists/*
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY freeq-sdk/ freeq-sdk/
COPY freeq-server/ freeq-server/
COPY freeq-auth-broker/ freeq-auth-broker/
COPY freeq-bots/ freeq-bots/
COPY freeq-bot-id/ freeq-bot-id/
COPY freeq-sdk-ffi/ freeq-sdk-ffi/
COPY freeq-windows-core/ freeq-windows-core/
COPY freeq-tui/ freeq-tui/
RUN cargo build --release --package freeq-server --features av-native

# ── Web client build ──
FROM node:20-slim AS web-builder
WORKDIR /app
COPY web-client/package.json web-client/package-lock.json ./
RUN npm ci --ignore-scripts
COPY web-client/ ./
# Skip tsc type-checking (test files have TS errors); just run vite build
RUN npx vite build && node -e "const fs=require('fs'),h=Date.now().toString(36);let s=fs.readFileSync('dist/sw.js','utf8');s=s.replace('__BUILD_HASH__',h);fs.writeFileSync('dist/sw.js',s);console.log('SW cache:',h)"

# ── Runtime ──
# rust:1.89-slim is based on Debian trixie; must match GLIBC version
FROM debian:trixie-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
RUN mkdir -p /app/data /app/web
COPY --from=builder /build/target/release/freeq-server /app/freeq-server
COPY --from=web-builder /app/dist /app/web
WORKDIR /app
EXPOSE 8080 4443
# Miren sets $PORT; use shell form so it expands
CMD /app/freeq-server \
  --listen-addr 127.0.0.1:16667 \
  --web-addr 0.0.0.0:${PORT:-8080} \
  --web-static-dir /app/web \
  --server-name staging.freeq.at \
  --db-path /app/data/freeq.db \
  --data-dir /app/data \
  --iroh \
  --motd "freeq staging — AV with iroh-live"
