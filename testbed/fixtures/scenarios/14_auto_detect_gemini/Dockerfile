# Test scenario: Gemini LLM auto-detection from GEMINI_API_KEY environment variable
# This scenario verifies that gola can automatically detect and configure Google Gemini
# when no LLM configuration is provided in the YAML file, relying solely on environment variables.

FROM alpine:latest

# Install basic dependencies
RUN apk update && apk add --no-cache \
    ca-certificates

# Create necessary directories
RUN mkdir -p /test /usr/local/bin

# Copy the compiled binaries
COPY target-cache/x86_64-unknown-linux-musl/release/gola /usr/local/bin/gola
RUN chmod +x /usr/local/bin/gola

COPY target-cache/x86_64-unknown-linux-musl/release/gola-test-harness /usr/local/bin/harness
RUN chmod +x /usr/local/bin/harness

# Set the working directory
WORKDIR /test