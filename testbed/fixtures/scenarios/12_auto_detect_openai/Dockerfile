# This Dockerfile creates a scenario to test automatic LLM provider detection
# based on environment variables only, with minimal configuration.
FROM alpine:latest

# Install minimal dependencies
RUN apk update && apk add --no-cache \
    ca-certificates

RUN mkdir -p /test /usr/local/bin

# Copy the pre-compiled gola application from the build context.
COPY target-cache/x86_64-unknown-linux-musl/release/gola /usr/local/bin/gola
RUN chmod +x /usr/local/bin/gola

# Copy the pre-compiled test harness from the build context.
COPY target-cache/x86_64-unknown-linux-musl/release/gola-test-harness /usr/local/bin/harness
RUN chmod +x /usr/local/bin/harness

# No API key set in Dockerfile - will be provided via docker run environment variables