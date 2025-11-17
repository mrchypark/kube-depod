# ---- Build stage ----
FROM rust:1.89-bookworm AS builder

WORKDIR /app

# 1) Leverage Docker layer cache for dependencies
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() { println!(\"dummy\"); }" > src/main.rs
RUN cargo build --release && rm -rf src

# 2) Build actual application
COPY . .
RUN cargo build --release --locked

# ---- Runtime stage ----
FROM gcr.io/distroless/cc-debian12 AS runtime

WORKDIR /app

COPY --from=builder /app/target/release/operator /app/operator

ENTRYPOINT ["/app/operator"]