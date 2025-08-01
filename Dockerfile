# Multi-stage build for smaller final image
FROM rust:1.88-slim AS builder

# Install system dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    git \
    build-essential \
    make \
    && rm -rf /var/lib/apt/lists/*

# Create app directory
WORKDIR /usr/src/app

# Copy the entire source first
COPY . .

# Build the application
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Create a non-root user
RUN useradd -r -s /bin/false arbitrage

# Create app directory
WORKDIR /app

# Copy the binary from builder stage
COPY --from=builder /usr/src/app/target/release/arbitrage-bot /app/arbitrage-bot

# Copy config example
COPY config.example.toml /app/config.example.toml

# Change ownership to non-root user
RUN chown -R arbitrage:arbitrage /app

# Switch to non-root user
USER arbitrage

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
  CMD /app/arbitrage-bot version || exit 1

# Default command
ENTRYPOINT ["/app/arbitrage-bot"]
CMD ["run"]