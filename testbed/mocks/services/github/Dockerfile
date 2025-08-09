# Multi-stage build for github-mock
FROM rust:1.71-alpine AS builder

# Install musl dev tools for static linking
RUN apk add --no-cache musl-dev pkgconfig openssl-dev

# Set working directory
WORKDIR /build

# Copy just the github-mock crate
COPY Cargo.toml ./
COPY src ./src

# Build the github-mock binary
RUN cargo build --release --target x86_64-unknown-linux-musl --bin github-mock

# Runtime stage - minimal Alpine Linux
FROM alpine:3.18

# Install curl and ca-certificates for health checks and HTTPS
RUN apk update && apk add --no-cache \
    ca-certificates \
    curl

# Create app directory
RUN mkdir -p /app /usr/local/bin

# Copy the binary from builder stage
COPY --from=builder /build/target/x86_64-unknown-linux-musl/release/github-mock /usr/local/bin/github-mock

# Make binary executable
RUN chmod +x /usr/local/bin/github-mock

# Set working directory
WORKDIR /app

# Expose both HTTP and HTTPS ports
EXPOSE 80 443

# Health check using HTTP endpoint
HEALTHCHECK --interval=5s --timeout=3s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:80/health || exit 1

# Run the github-mock server
CMD ["/usr/local/bin/github-mock"]