# This Dockerfile creates a scenario where MCP server setup fails due to missing npm/npx
# It assumes binaries have been pre-compiled into the `target-cache` directory.
FROM alpine:latest

RUN mkdir -p /test /usr/local/bin

# Copy the pre-compiled gola application from the build context.
COPY target-cache/x86_64-unknown-linux-musl/release/gola /usr/local/bin/gola
RUN chmod +x /usr/local/bin/gola

# Copy the pre-compiled test harness from the build context.
COPY target-cache/x86_64-unknown-linux-musl/release/gola-test-harness /usr/local/bin/harness
RUN chmod +x /usr/local/bin/harness

# Create fake npx and npm commands that exit with an error to simulate missing Node.js
RUN echo -e '#!/bin/sh\necho "npx command not found" >&2\nexit 1' > /usr/local/bin/npx && chmod +x /usr/local/bin/npx
RUN echo -e '#!/bin/sh\necho "npm command not found" >&2\nexit 1' > /usr/local/bin/npm && chmod +x /usr/local/bin/npm