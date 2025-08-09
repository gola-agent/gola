FROM alpine:latest

# Create test directory
RUN mkdir -p /test /usr/local/bin

# Copy gola binary
COPY target-cache/x86_64-unknown-linux-musl/release/gola /usr/local/bin/gola
RUN chmod +x /usr/local/bin/gola

# Copy test harness
COPY target-cache/x86_64-unknown-linux-musl/release/gola-test-harness /usr/local/bin/harness
RUN chmod +x /usr/local/bin/harness

# Note: This environment deliberately does NOT have Python installed
# to test gola's behavior when python is missing