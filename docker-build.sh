#!/bin/bash

# Build the Rust library in a Docker container
docker run --rm -it \
    -v "$(pwd):/usr/src/schematic_utils" \
    --platform linux/arm64 \
    rust:1.74-slim-bullseye \
    bash -c '
        apt-get update &&
        apt-get install -y build-essential gcc-aarch64-linux-gnu &&
        cd /usr/src/schematic_utils &&
        cargo build --release --target aarch64-unknown-linux-gnu --features ffi
    '
