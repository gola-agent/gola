FROM alpine:latest

# Create test directory
RUN mkdir -p /test /usr/local/bin

# Copy gola binary
COPY target-cache/x86_64-unknown-linux-musl/release/gola /usr/local/bin/gola
RUN chmod +x /usr/local/bin/gola

# Copy test harness
COPY target-cache/x86_64-unknown-linux-musl/release/gola-test-harness /usr/local/bin/harness
RUN chmod +x /usr/local/bin/harness

# Copy test documents
# Documents directory is in the same directory as this Dockerfile
# The build is run from integration_tests with -f pointing to this Dockerfile
# So we need to reference from integration_tests directory
RUN mkdir -p /test/documents
# We'll mount the documents at runtime instead