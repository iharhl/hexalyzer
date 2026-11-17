#!/usr/bin/env bash
#
# Project task runner.
#
# Usage: ./run.sh <action>
#   check   Format the code and run clippy with strict lints
#   test    Run CLI tests, doc tests, and unit/integration tests with coverage
#   bench   Run the intelhexlib benchmarks

set -euo pipefail

# Always run from the repository root, wherever the script is called from
cd "$(dirname "$0")"

usage() {
    echo "Usage: $0 <check|test|bench>" >&2
    exit 1
}

run_check() {
    cargo fmt

    # Lints are configured in `[workspace.lints]` of the root Cargo.toml
    cargo clippy --workspace --all-targets --all-features
}

run_test() {
    # Run CLI tests
    cargo test -p intelhexlib --features cli --test cli_tests

    # Run doc tests
    cargo test -p intelhexlib --doc

    # Run tests with report coverage.
    # Only runs unit and integration tests for `intelhexlib`, excluding CLI.
    cargo llvm-cov -p intelhexlib
}

run_bench() {
    cargo bench -p intelhexlib --bench benchmark --features benchmarking
}

[[ $# -eq 1 ]] || usage

case "$1" in
    check) run_check ;;
    test) run_test ;;
    bench) run_bench ;;
    *) usage ;;
esac
