# Build Stage
FROM rust:1.98.1-slim-bookworm AS builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy the entire workspace
COPY . .

# Build the application
RUN cargo build --release --locked --bin conxian-nexus

# Runtime Stage
FROM debian:bookworm-slim

WORKDIR /app

# Install runtime dependencies. redis-server is bundled as a local sidecar so a
# single container can satisfy Nexus's Redis cache dependency (REDIS_URL). A
# managed/remote Redis is the production hardening path; see docker-compose.yml.
RUN apt-get update && apt-get install -y \
    libssl3 \
    ca-certificates \
    redis-server \
    && rm -rf /var/lib/apt/lists/*

# Copy the binary from the builder
COPY --from=builder /app/target/release/conxian-nexus /usr/local/bin/conxian-nexus
COPY --from=builder /app/migrations /app/migrations
COPY docker-entrypoint.sh /usr/local/bin/docker-entrypoint.sh
RUN chmod +x /usr/local/bin/docker-entrypoint.sh

# Expose REST and gRPC ports
EXPOSE 3000
EXPOSE 50051

# Set default environment variables
ENV RUST_LOG=info
ENV GRPC_PORT=50051

ENTRYPOINT ["docker-entrypoint.sh"]
