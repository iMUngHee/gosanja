#!/bin/sh
set -eu

cd "$(dirname "$0")/.."
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace --all-targets
cargo test --locked --workspace --doc
cargo deny --locked check
