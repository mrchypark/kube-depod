# ---- Build stage ----
FROM rust:1.89 AS builder

# 1) 'TARGETARCH'는 buildx에 의해 'amd64' 또는 'arm64'로 자동 설정됩니다.
ARG TARGETARCH

# 2) C 컴파일러 및 Rust 타겟 설치
RUN apt-get update && apt-get install -y musl-tools cmake perl pkg-config && \
    rustup target add x86_64-unknown-linux-musl && \
    rustup target add aarch64-unknown-linux-musl

WORKDIR /app

# 3) 아키텍처에 맞게 더미 빌드
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() { println!(\"dummy\"); }" > src/main.rs
RUN if [ "$TARGETARCH" = "amd64" ]; then \
        cargo build --release --target x86_64-unknown-linux-musl; \
    elif [ "$TARGETARCH" = "arm64" ]; then \
        cargo build --release --target aarch64-unknown-linux-musl; \
    fi && rm -rf src

# 4) 아키텍처에 맞게 실제 앱 빌드
COPY . .
RUN if [ "$TARGETARCH" = "amd64" ]; then \
        cargo build --release --locked --target x86_64-unknown-linux-musl; \
    elif [ "$TARGETARCH" = "arm64" ]; then \
        cargo build --release --locked --target aarch64-unknown-linux-musl; \
    fi

# 5) [중요] 빌드된 바이너리를 아키텍처와 상관없이 동일한 경로로 이동
RUN if [ "$TARGETARCH" = "amd64" ]; then \
        mkdir -p /out && mv /app/target/x86_64-unknown-linux-musl/release/operator /out/operator; \
    elif [ "$TARGETARCH" = "arm64" ]; then \
        mkdir -p /out && mv /app/target/aarch64-unknown-linux-musl/release/operator /out/operator; \
    fi

# ---- Runtime stage ----
# 'static' 이미지는 아키텍처별로 존재합니다.
FROM gcr.io/distroless/static-debian12 AS runtime

# ca-certs와 tzdata 복사 (builder의 아키텍처와 무관)
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt
COPY --from=builder /usr/share/zoneinfo /usr/share/zoneinfo

WORKDIR /app

# 6) [중요] 일관된 경로에서 바이너리 복사
COPY --from=builder /out/operator /app/operator

ENTRYPOINT ["/app/operator"]