# This Dockerfile is for the two-step build process.
# It assumes binaries have been pre-compiled into the `target-cache` directory.
FROM alpine:latest

RUN mkdir -p /test /usr/local/bin

# Copy the pre-compiled gola application from the build context.
COPY target-cache/x86_64-unknown-linux-musl/release/gola /usr/local/bin/gola
RUN chmod +x /usr/local/bin/gola

# Copy the pre-compiled test harness from the build context.
COPY target-cache/x86_64-unknown-linux-musl/release/gola-test-harness /usr/local/bin/harness
RUN chmod +x /usr/local/bin/harness