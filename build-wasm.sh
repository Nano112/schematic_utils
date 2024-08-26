#!/bin/bash

# Build the Wasm module
wasm-pack build --target web

# Check if build was successful
if [ $? -ne 0 ]; then
    echo "Wasm build failed"
    exit 1
fi

# Copy the files to the wasm-test directory
cp -r pkg/*.js pkg/*.wasm wasm-test/

# Check if copy was successful
if [ $? -ne 0 ]; then
    echo "Failed to copy Wasm files to wasm-test directory"
    exit 1
fi

echo "Wasm build and copy completed successfully"