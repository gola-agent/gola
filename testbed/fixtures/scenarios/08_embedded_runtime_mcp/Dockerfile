# This Dockerfile creates a scenario where MCP server succeeds using embedded runtime
# to download Python on demand. Starts with minimal Alpine base without Python.
FROM alpine:latest

# Install essential dependencies for embedded runtime to work
# (curl/wget for downloads, ca-certificates for HTTPS, tar for extraction, bash for scripting)
RUN apk update && apk add --no-cache \
    curl \
    ca-certificates \
    tar \
    gzip \
    bash \
    unzip

RUN mkdir -p /test /usr/local/bin

# Copy the pre-compiled gola application from the build context.
COPY target-cache/x86_64-unknown-linux-musl/release/gola /usr/local/bin/gola
RUN chmod +x /usr/local/bin/gola

# Copy the pre-compiled test harness from the build context.
COPY target-cache/x86_64-unknown-linux-musl/release/gola-test-harness /usr/local/bin/harness
RUN chmod +x /usr/local/bin/harness

# Verify Python is NOT initially available (should fail)
RUN python3 --version || echo "Python not initially available - will be downloaded by embedded runtime"