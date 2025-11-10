# Multi-stage build for minimal image size
FROM rust:1.75 as builder

WORKDIR /build

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Create dummy main.rs to cache dependencies
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    cargo build --release && \
    rm -rf src

# Copy actual source code
COPY src ./src

# Build for release
RUN cargo build --release

# Runtime stage - use slim debian image
FROM debian:bookworm-slim

# Install CA certificates for HTTPS
RUN apt-get update && \
    apt-get install -y ca-certificates && \
    rm -rf /var/lib/apt/lists/*

# Create app directory and user
RUN useradd -m -u 1000 collector && \
    mkdir -p /app/data && \
    chown -R collector:collector /app

WORKDIR /app

# Copy binary from builder
COPY --from=builder /build/target/release/office_audit_log_collector /app/

# Switch to non-root user
USER collector

# Set working directory for data files
VOLUME ["/app/data"]

# Entry point
ENTRYPOINT ["/app/office_audit_log_collector"]

# Default command (can be overridden)
CMD ["--config", "/app/config.yaml"]
