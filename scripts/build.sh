#!/bin/bash
set -e

echo "Building Nendi release binary..."
cargo build --release -p nendi

echo "Build complete: target/release/nendi"
