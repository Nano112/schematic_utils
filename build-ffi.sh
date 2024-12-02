#!/bin/bash

# Build the Rust library in a Docker container
docker run --rm -it \
    -v "$(pwd):/usr/src/schematic_utils" \
    rust:1.74-slim \
    bash -c 'cd /usr/src/schematic_utils && cargo build --release --features ffi'

## After the build completes, copy the library to your Laravel project
#cp target/release/libminecraft_schematic_utils.so ../schemati/public/minecraft_schematic_utils.so
#chmod 755 ../schemati/public/minecraft_schematic_utils.so
#
#echo "Build complete! Library has been copied to your Laravel project's public directory."