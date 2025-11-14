# ---- Build stage ----
# 런타임(debian12)과 맞추기 위해 bookworm 기반 빌더 사용
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
# 'cc-debian12'는 glibc 2.36을 포함하며, 
# ca-certificates, tzdata, libssl 등도 이미 포함되어 있습니다.
FROM gcr.io/distroless/cc-debian12 AS runtime

WORKDIR /app

# 빌더에서 바이너리 복사
COPY --from=builder /app/target/release/operator /app/operator

# Distroless 이미지는 기본적으로 'nonroot' 유저(UID 65532)로 실행됩니다.
# (기존 Dockerfile의 'appuser'와 유사한 목적 달성)
# USER 65532:65532 

# ENTRYPOINT로 실행 파일 지정 (CMD 대신)
ENTRYPOINT ["/app/operator"]