# VAOS-Crucible Dockerfile
# Multi-stage build for Rust eBPF service
# IMPORTANT: eBPF requires Linux kernel and privileged container

# Build stage
FROM rust:1.83-alpine AS builder

WORKDIR /app

# Install build dependencies for eBPF
RUN apk add --no-cache \
    build-base \
    clang \
    llvm \
    llvm-dev \
    elfutils-dev \
    zlib-dev \
    libelf-dev \
    linux-headers \
    bpf-tools \
    git

# Copy Cargo files
COPY Cargo.toml Cargo.lock ./

# Create dummy main.rs to cache dependencies
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    cargo fetch --locked && \
    rm -rf src

# Copy source code
COPY . .

# Build the application
RUN cargo build --release

# Runtime stage
FROM alpine:latest

WORKDIR /app

# Install runtime dependencies for eBPF
RUN apk add --no-cache \
    libelf \
    libstdc++ \
    clang \
    llvm-libs \
    bash \
    iproute2 \
    iptables

# Copy binary from builder
COPY --from=builder /app/target/release/crucible .

# Create non-root user (but container will need to be privileged)
RUN addgroup -g 1000 crucible && \
    adduser -D -u 1000 -G crucible crucible && \
    chown -R crucible:crucible /app

USER crucible

# Expose ports
# 50052: gRPC server
EXPOSE 50052

# Health check
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
  CMD wget --no-verbose --tries=1 --spider http://localhost:50052/health || exit 1

# Run the application
# Note: Container must be run with --privileged and --cap-add=ALL for eBPF
CMD ["./crucible"]
