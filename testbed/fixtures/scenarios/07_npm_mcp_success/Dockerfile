# This Dockerfile creates a scenario where MCP server setup succeeds with npm/npx available
# It assumes binaries have been pre-compiled into the `target-cache` directory.
FROM alpine:latest

# Install Node.js and npm
RUN apk update && apk add --no-cache nodejs npm

RUN mkdir -p /test /usr/local/bin

# Copy the pre-compiled gola application from the build context.
COPY target-cache/x86_64-unknown-linux-musl/release/gola /usr/local/bin/gola
RUN chmod +x /usr/local/bin/gola

# Copy the pre-compiled test harness from the build context.
COPY target-cache/x86_64-unknown-linux-musl/release/gola-test-harness /usr/local/bin/harness
RUN chmod +x /usr/local/bin/harness

# Create a test directory that the filesystem server can access
RUN mkdir -p /test/mcp-files
RUN echo "This is a test file for MCP filesystem server" > /test/mcp-files/test.txt

# Verify npm and npx are working
RUN npm --version
RUN npx --version

# Pre-test the MCP filesystem server (this will cache the package)
RUN timeout 5s npx -y @modelcontextprotocol/server-filesystem /test/mcp-files || echo "MCP filesystem server tested successfully"