# Build Stage
FROM rust:1.82-slim AS builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy the entire workspace
COPY . .

# Build the application
RUN cargo build --release

# Runtime Stage
FROM debian:bookworm-slim

WORKDIR /app

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    libssl3 \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Copy the binary from the builder
COPY --from=builder /app/target/release/conxian-nexus /usr/local/bin/conxian-nexus
COPY --from=builder /app/migrations /app/migrations

# Expose REST and gRPC ports
EXPOSE 3000
EXPOSE 50051

# Set default environment variables
ENV RUST_LOG=info
ENV REST_PORT=3000
ENV GRPC_PORT=50051

CMD ["conxian-nexus"]
