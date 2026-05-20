FROM node:22-alpine AS frontend-builder

WORKDIR /app/admin-ui
COPY admin-ui/package.json ./
RUN npm install -g pnpm && pnpm install --ignore-scripts
COPY admin-ui ./
RUN pnpm build

WORKDIR /app/user-ui
COPY user-ui/package.json ./
RUN npm install
COPY user-ui ./
RUN npm run build

FROM rust:1-alpine AS builder

RUN apk add --no-cache musl-dev openssl-dev openssl-libs-static

WORKDIR /app
COPY Cargo.toml Cargo.lock* ./
COPY src ./src
COPY --from=frontend-builder /app/admin-ui/dist /app/admin-ui/dist
COPY --from=frontend-builder /app/user-ui/dist /app/user-ui/dist

RUN cargo build --release

FROM alpine:3.21

RUN apk add --no-cache ca-certificates

WORKDIR /app
COPY --from=builder /app/target/release/kiro-rs /app/kiro-rs

EXPOSE 5678

CMD sh -c 'mkdir -p /app/config && \
  if [ ! -f /app/config/config.json ]; then \
    echo "{\"apiKey\":\"${API_KEY}\",\"host\":\"${HOST:-0.0.0.0}\",\"port\":${PORT:-5678},\"adminApiKey\":\"${ADMIN_API_KEY}\"}" > /app/config/config.json; \
  fi && \
  if [ ! -f /app/config/credentials.json ]; then \
    echo "[]" > /app/config/credentials.json; \
  fi && \
  ./kiro-rs --config /app/config/config.json --credentials /app/config/credentials.json'
