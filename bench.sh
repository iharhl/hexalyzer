#!/bin/bash

echo "Running all benchmarks..."

# Run intelhexlib benches
cargo bench -p intelhexlib --bench benchmark --features benchmarking
