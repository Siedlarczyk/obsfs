# =============================================================================
# ObsFS Docker Image
# =============================================================================
# Multi-stage build for minimal final image
#
# Build:
#   docker build -t obsfs -f pkg/docker/Dockerfile .
#
# Run (requires privileged mode for FUSE):
#   docker run --rm -it --privileged obsfs
#
# Or with explicit capabilities:
#   docker run --rm -it --cap-add SYS_ADMIN --device /dev/fuse obsfs
# =============================================================================

# -----------------------------------------------------------------------------
# Stage 1: Build
# -----------------------------------------------------------------------------
FROM rust:1.77-bookworm AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libfuse3-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Cache dependencies by copying only Cargo files first
COPY Cargo.toml Cargo.lock ./
COPY crates/obsfs-core/Cargo.toml crates/obsfs-core/
COPY crates/obsfs-fuse/Cargo.toml crates/obsfs-fuse/
COPY crates/obsfs-plugins/Cargo.toml crates/obsfs-plugins/
COPY crates/obsfs-cli/Cargo.toml crates/obsfs-cli/

# Create dummy source files to build dependencies
RUN mkdir -p crates/obsfs-core/src crates/obsfs-fuse/src crates/obsfs-plugins/src crates/obsfs-cli/src && \
    echo "pub fn dummy() {}" > crates/obsfs-core/src/lib.rs && \
    echo "pub fn dummy() {}" > crates/obsfs-fuse/src/lib.rs && \
    echo "pub fn dummy() {}" > crates/obsfs-plugins/src/lib.rs && \
    echo "fn main() {}" > crates/obsfs-cli/src/main.rs

# Build dependencies (this layer is cached)
RUN cargo build --release && rm -rf target/release/.fingerprint/obsfs*

# Copy actual source code
COPY crates crates

# Build the final binary
RUN cargo build --release --locked

# -----------------------------------------------------------------------------
# Stage 2: Runtime
# -----------------------------------------------------------------------------
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    fuse3 \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && echo "user_allow_other" >> /etc/fuse.conf

# Copy binary from builder
COPY --from=builder /build/target/release/obsfs /usr/local/bin/obsfs

# Copy default config
COPY config/default.toml /etc/obsfs/config.toml

# Create mount point
RUN mkdir -p /obs

# Labels
LABEL org.opencontainers.image.title="ObsFS"
LABEL org.opencontainers.image.description="Observability Filesystem - Observe everything as files"
LABEL org.opencontainers.image.source="https://github.com/Siedlarczyk/obsfs"
LABEL org.opencontainers.image.licenses="MIT"

# Environment
ENV RUST_LOG=warn

# Health check
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD test -d /obs/system || exit 1

# Default command
ENTRYPOINT ["/usr/local/bin/obsfs"]
CMD ["mount", "/obs", "--allow-other"]
