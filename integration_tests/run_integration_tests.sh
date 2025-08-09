#!/bin/bash

# This script tests the real gola application using the comprehensive,
# compatible caching method.
set -e

echo "--- Starting Integration Test for scenario: $SCENARIO_NAME ---"

# --- 1. Define Paths & Names ---
SCENARIO_NAME="${1:-04_basic_mcp}"
SCENARIO_DIR="../testbed/fixtures/scenarios/$SCENARIO_NAME"
DOCKERFILE_PATH="$SCENARIO_DIR/Dockerfile"
DOCKER_COMPOSE_PATH="$SCENARIO_DIR/docker-compose.yml"
SCENARIO_YAML="scenario.yaml"
GOLA_CONFIG="gola.yaml"
DOCKER_IMAGE_NAME="gola-test-app-cached-$SCENARIO_NAME"
BUILD_IMAGE="messense/rust-musl-cross:x86_64-musl"
HOST_TARGET_CACHE_DIR="$(pwd)/target-cache"
HOST_REGISTRY_CACHE_DIR="$(pwd)/cargo-registry-cache"
HOST_GIT_CACHE_DIR="$(pwd)/cargo-git-cache"

# --- 2. Build Step (with full caching) ---
echo "Ensuring cache directories exist..."
mkdir -p $HOST_TARGET_CACHE_DIR
mkdir -p $HOST_REGISTRY_CACHE_DIR
mkdir -p $HOST_GIT_CACHE_DIR

# Check if this is a Docker Compose scenario
if [ -f "$DOCKER_COMPOSE_PATH" ]; then
  echo "Docker Compose scenario detected, building with github-mock..."
  PACKAGES="gola-cli gola-test-harness"
else
  echo "Standard scenario, building gola and harness..."
  PACKAGES="gola-cli gola-test-harness"
fi

echo "Compiling $PACKAGES with full cache..."
docker run --rm \
  --platform linux/amd64 \
  -v "$(pwd)/..:/volume" \
  -v "$HOST_TARGET_CACHE_DIR:/volume/target" \
  -v "$HOST_REGISTRY_CACHE_DIR:/root/.cargo/registry" \
  -v "$HOST_GIT_CACHE_DIR:/root/.cargo/git" \
  -w "/volume" \
  $BUILD_IMAGE \
  cargo build --release --target x86_64-unknown-linux-musl $(for pkg in $PACKAGES; do echo "--package $pkg"; done)

# Build github-mock if needed
if [ -f "$DOCKER_COMPOSE_PATH" ]; then
  echo "Building github-mock..."
  docker run --rm \
    --platform linux/amd64 \
    -v "$(pwd)/..:/volume" \
    -v "$HOST_TARGET_CACHE_DIR:/volume/target" \
    -v "$HOST_REGISTRY_CACHE_DIR:/root/.cargo/registry" \
    -v "$HOST_GIT_CACHE_DIR:/root/.cargo/git" \
    -w "/volume" \
    $BUILD_IMAGE \
    cargo build --release --target x86_64-unknown-linux-musl --package github-mock
fi

# --- 3. Package Step ---
if [ -f "$DOCKER_COMPOSE_PATH" ]; then
  echo "Using Docker Compose for multi-service scenario..."
  cd "$SCENARIO_DIR"
  
  # Run Docker Compose
  echo "Starting services with Docker Compose..."
  OPENAI_API_KEY="$OPENAI_API_KEY" \
  ANTHROPIC_API_KEY="$ANTHROPIC_API_KEY" \
  GEMINI_API_KEY="$GEMINI_API_KEY" \
  docker compose up --build --abort-on-container-exit --exit-code-from gola-test
  
  # Cleanup
  echo "Cleaning up Docker Compose services..."
  docker compose down --volumes --remove-orphans
  
  cd - > /dev/null
else
  echo "Building final test image..."
  docker buildx build --platform linux/amd64 --load -t $DOCKER_IMAGE_NAME -f $DOCKERFILE_PATH .

  # --- 4. Run Test in Container ---
  # The timeout is now handled by the harness, so we can run the container directly.
  echo "Running Rust harness inside the container..."
  # Check if we need to mount documents for RAG test
  if [ -d "$SCENARIO_DIR/documents" ]; then
    DOCUMENTS_MOUNT="-v $(realpath $SCENARIO_DIR)/documents:/test/documents:ro"
  else
    DOCUMENTS_MOUNT=""
  fi
  
  docker run --rm --platform linux/amd64 \
    -e OPENAI_API_KEY \
    -e ANTHROPIC_API_KEY \
    -e GEMINI_API_KEY \
    -v "$(realpath $SCENARIO_DIR)/$SCENARIO_YAML:/test/scenario.yaml:ro" \
    -v "$(realpath $SCENARIO_DIR)/$GOLA_CONFIG:/test/gola.yaml:ro" \
    -v "$(realpath $SCENARIO_DIR)/prompts:/test/prompts:ro" \
    $DOCUMENTS_MOUNT \
    $DOCKER_IMAGE_NAME \
    "/usr/local/bin/harness" "/test/scenario.yaml"
fi

# --- 5. Report Result ---
echo ""
echo "✅ Scenario $SCENARIO_NAME Passed"
echo "--- Integration Test Finished ---"