# 1. Build do Rust e CSS
FROM rust:1.72 AS builder
WORKDIR /app
COPY . .
RUN cargo build --release

# Tailwind build
FROM node:18-alpine AS css-build
WORKDIR /app
COPY package*.json ./
RUN npm ci
COPY . .
RUN npm run build-css-prod

# 2. Imagem final leve
FROM debian:bullseye-slim
WORKDIR /app

# Binário Rust
COPY --from=builder /app/target/release/koshelf /app/koshelf
# CSS e assets
COPY --from=css-build /app/assets /app/assets
COPY templates /app/templates
# Config e pastas necessárias

EXPOSE 3000

CMD ["/app/koshelf"]
