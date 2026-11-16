# ---- editor (Solid + Vite) ----
FROM node:22-bookworm-slim AS editor
WORKDIR /editor
COPY editor/package.json editor/package-lock.json ./
RUN npm ci --no-audit --no-fund
COPY editor/ ./
RUN npm run build

# ---- api (Rust) ----
FROM rust:1-slim-bookworm AS api
WORKDIR /api
# Byg afhængighederne i et lag for sig, så de kun genbygges når Cargo.lock ændres
COPY api/Cargo.toml api/Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs && cargo build --release && rm -rf src
COPY api/ ./
RUN touch src/main.rs && cargo build --release

# ---- runtime ----
FROM debian:bookworm-slim
WORKDIR /app
COPY --from=api /api/target/release/webmaster-api /app/webmaster-api
COPY --from=editor /editor/dist /app/static
COPY definitions /app/definitions
COPY seed.json /app/seed.json
ENV STATIC_DIR=/app/static \
    DEFINITIONS_DIR=/app/definitions \
    SEED_FILE=/app/seed.json \
    BIND=0.0.0.0:8080
EXPOSE 8080
CMD ["/app/webmaster-api"]
