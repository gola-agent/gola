# This Dockerfile creates a scenario where MCP server succeeds using embedded runtime
# to download Bun on demand. Starts with minimal Alpine base without Node.js/Bun.
FROM alpine:latest

# Install essential dependencies for embedded runtime to work
# (curl/wget for downloads, ca-certificates for HTTPS, tar for extraction, bash for scripting)
# Also install libstdc++ and libgcc which are needed for bun binary
RUN apk update && apk add --no-cache \
    curl \
    ca-certificates \
    tar \
    gzip \
    bash \
    unzip \
    libstdc++ \
    libgcc

RUN mkdir -p /test /usr/local/bin

# Copy the pre-compiled gola application from the build context.
COPY target-cache/x86_64-unknown-linux-musl/release/gola /usr/local/bin/gola
RUN chmod +x /usr/local/bin/gola

# Copy the pre-compiled test harness from the build context.
COPY target-cache/x86_64-unknown-linux-musl/release/gola-test-harness /usr/local/bin/harness
RUN chmod +x /usr/local/bin/harness

# Verify Node.js/Bun is NOT initially available (should fail)
RUN node --version || echo "Node.js not initially available - will be downloaded by embedded runtime"
RUN bun --version || echo "Bun not initially available - will be downloaded by embedded runtime"

# Create a test directory that the filesystem server can access
RUN mkdir -p /test/mcp-files
RUN echo "Hello from embedded bun runtime!" > /test/mcp-files/hello.txt  
RUN echo "This is a test file for MCP filesystem server" > /test/mcp-files/test.md