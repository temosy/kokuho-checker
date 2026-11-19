# kokuho-checker web — static musl build on Alpine.
# Runs behind the temosy-wordpress stack's nginx at temosy.com/kokuho/
# (see that repo's compose.yaml; this image itself is deployment-agnostic).
FROM docker.io/library/rust:1-alpine AS build
RUN apk add --no-cache musl-dev
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --bin web

FROM docker.io/library/alpine:3.22
# The base image lags the Alpine repo (2026-11-19: zlib 1.3.2 left behind).
# Upgrade first so system-kakari's container_refresh.py "stale packages" check clears after a rebuild.
RUN apk upgrade --no-cache && adduser -D -H kokuho
WORKDIR /app
COPY --from=build /src/target/release/web /usr/local/bin/kokuho-web
COPY data /app/data
USER kokuho
ENV KOKUHO_BIND=0.0.0.0 \
    KOKUHO_PORT=8787 \
    KOKUHO_DATA_DIR=/app/data
EXPOSE 8787
CMD ["kokuho-web"]
