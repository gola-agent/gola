FROM rustlang/rust:nightly-bookworm AS builder

WORKDIR /volume

COPY Cargo.toml Cargo.lock ./
COPY crates/ ./crates/
COPY testbed/ ./testbed/

# Build the gola binary
RUN cargo build --release --package gola-cli && \
    cp target/release/gola /volume/gola

FROM python:3.12-slim-bookworm

RUN apt-get update && apt-get install -y \
    ca-certificates \
    tzdata \
    curl \
    git \
    && curl -fsSL https://deb.nodesource.com/setup_lts.x | bash - \
    && apt-get install -y nodejs \
    && rm -rf /var/lib/apt/lists/* \
    && curl -LsSf https://astral.sh/uv/install.sh | UV_INSTALL_DIR=/usr/local/bin sh

RUN groupadd -g 1000 gola && \
    useradd -d /home/gola -s /bin/bash -u 1000 -g gola gola

RUN mkdir -p /app /data /home/gola && \
    chown -R gola:gola /app /data /home/gola

COPY --from=builder /volume/gola /usr/local/bin/gola
RUN chmod +x /usr/local/bin/gola

USER gola
WORKDIR /data

EXPOSE 3001

CMD ["gola", "--server-only"]

HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
  CMD gola --help || exit 1

LABEL maintainer="Gianluca Brigandi" \
      description="Gola - Prompt-defined LLM Agents" \
      version="0.1.0"
