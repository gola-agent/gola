#!/bin/bash

# Docker build script for Gola CLI
# Uses Debian-based multi-arch build for UV Python compatibility

set -e

echo "--- Building Gola Docker Image with Multi-Arch Support ---"

# --- 1. Define Paths & Names ---
HOST_TARGET_CACHE_DIR="$(pwd)/target-cache"
HOST_REGISTRY_CACHE_DIR="$(pwd)/cargo-registry-cache"
HOST_GIT_CACHE_DIR="$(pwd)/cargo-git-cache"
DOCKER_IMAGE_NAME="gola:latest"

# --- 2. Build Step (multi-arch compatible) ---
echo "Ensuring cache directories exist..."
mkdir -p $HOST_TARGET_CACHE_DIR
mkdir -p $HOST_REGISTRY_CACHE_DIR
mkdir -p $HOST_GIT_CACHE_DIR

# --- 3. Build Docker image with caching for faster subsequent builds ---
echo "Building Docker image with UV Python compatibility and dependency caching..."
# Enable BuildKit for advanced caching features
export DOCKER_BUILDKIT=1
docker build -t $DOCKER_IMAGE_NAME .

# --- 4. Report Result ---
echo ""
echo "✅ Gola Docker image built successfully: $DOCKER_IMAGE_NAME"
echo ""
echo "Usage:"
echo "  docker run -d -p 3001:3001 -e OPENAI_API_KEY=your_key $DOCKER_IMAGE_NAME"
echo ""
echo "Cache directories (can be preserved for faster subsequent builds):"
echo "  - $HOST_TARGET_CACHE_DIR"
echo "  - $HOST_REGISTRY_CACHE_DIR" 
echo "  - $HOST_GIT_CACHE_DIR"
echo ""
echo "--- Build Complete ---"