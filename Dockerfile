# ---- Build stage ----
FROM rust:1.89 AS builder

WORKDIR /app

# 1) Leverage Docker layer cache for dependencies
#    (필요에 따라 workspace 구조에 맞게 Cargo.toml 경로 조정)
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() { println!(\"dummy\"); }" > src/main.rs
RUN cargo build --release && rm -rf src

# 2) Build actual application
COPY . .
RUN cargo build --release --locked

# ---- Runtime stage ----
FROM debian:bookworm-slim AS runtime

# Install runtime dependencies only (no build tools)
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates tzdata \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -u 1000 -m appuser
USER appuser

# Copy binary from builder image
COPY --from=builder /app/target/release/operator /usr/local/bin/operator

CMD ["operator"]
