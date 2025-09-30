FROM rustlang/rust:nightly AS builder
WORKDIR /app
COPY . .
RUN apt-get update && apt-get install -y nodejs npm
RUN cargo build --release

FROM node:18-alpine AS css-build
WORKDIR /app
COPY package*.json ./
RUN npm ci
COPY assets ./assets
RUN npm run build-css-prod

FROM debian:bullseye-slim
WORKDIR /app
COPY --from=builder /app/target/release/koshelf /app/koshelf
COPY --from=css-build /app/assets /app/assets
COPY templates /app/templates
EXPOSE 3000
CMD ["/app/koshelf"]
