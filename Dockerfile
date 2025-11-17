# ---- Build stage ----
FROM rust:1.89 AS builder

ARG TARGETARCH

RUN apt-get update && apt-get install -y musl-tools cmake perl pkg-config && \
  rustup target add x86_64-unknown-linux-musl && \
  rustup target add aarch64-unknown-linux-musl

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() { println!(\"dummy\"); }" > src/main.rs
RUN if [ "$TARGETARCH" = "amd64" ]; then \
  cargo build --release --target x86_64-unknown-linux-musl; \
  elif [ "$TARGETARCH" = "arm64" ]; then \
  cargo build --release --target aarch64-unknown-linux-musl; \
  fi && rm -rf src

COPY . .
RUN if [ "$TARGETARCH" = "amd64" ]; then \
  cargo build --release --locked --target x86_64-unknown-linux-musl; \
  elif [ "$TARGETARCH" = "arm64" ]; then \
  cargo build --release --locked --target aarch64-unknown-linux-musl; \
  fi

RUN if [ "$TARGETARCH" = "amd64" ]; then \
  mkdir -p /out && mv /app/target/x86_64-unknown-linux-musl/release/operator /out/operator; \
  elif [ "$TARGETARCH" = "arm64" ]; then \
  mkdir -p /out && mv /app/target/aarch64-unknown-linux-musl/release/operator /out/operator; \
  fi

# ---- Runtime stage ----
FROM gcr.io/distroless/static-debian12 AS runtime

COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt
COPY --from=builder /usr/share/zoneinfo /usr/share/zoneinfo

WORKDIR /app

COPY --from=builder /out/operator /app/operator

ENTRYPOINT ["/app/operator"]