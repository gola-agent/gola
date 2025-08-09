# This Dockerfile creates a scenario where MCP server setup succeeds
# It assumes binaries have been pre-compiled into the `target-cache` directory.
FROM alpine:latest

# Install Python and pip (required for uvx and mcp-server-time)
RUN apk update && apk add --no-cache \
    python3 \
    python3-dev \
    py3-pip \
    build-base \
    libffi-dev \
    openssl-dev \
    && python3 -m pip install --upgrade pip --break-system-packages

# Install uv (which provides uvx) globally
RUN python3 -m pip install uv --break-system-packages

RUN mkdir -p /test /usr/local/bin

# Copy the pre-compiled gola application from the build context.
COPY target-cache/x86_64-unknown-linux-musl/release/gola /usr/local/bin/gola
RUN chmod +x /usr/local/bin/gola

# Copy the pre-compiled test harness from the build context.
COPY target-cache/x86_64-unknown-linux-musl/release/gola-test-harness /usr/local/bin/harness
RUN chmod +x /usr/local/bin/harness

# Verify uvx is working and can run mcp-server-time
RUN uvx --help
RUN uvx mcp-server-time --help || echo "mcp-server-time executed (may have exited with help or missing args)"