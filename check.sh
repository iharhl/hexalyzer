cargo fmt

cargo clippy --workspace --all-targets --all-features -- \
  -W clippy::all \
  -W clippy::pedantic \
  -W clippy::nursery \
  -W clippy::unwrap_used \
  -W clippy::expect_used \
  -W clippy::panic
