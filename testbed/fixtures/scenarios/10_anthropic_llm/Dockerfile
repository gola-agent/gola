# This Dockerfile creates a scenario to test Anthropic LLM integration
# without any MCP servers configured, using minimal Alpine base.
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

# Set environment variable for Anthropic API key (will be overridden by docker run)
ENV ANTHROPIC_API_KEY=test_key_placeholder