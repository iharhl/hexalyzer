# Run CLI tests
cargo test -p intelhexlib --features cli --test cli_tests

# Run doc tests
cargo test -p intelhexlib --doc

# Run tests with report coverage.
# Only runs unit and integration tests for `intelhexlib`, excluding CLI.
cargo llvm-cov -p intelhexlib
