#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
# Requires cargo-audit and access to the current RustSec advisory database.
# Any missing tool, network failure, or advisory causes this gate to fail.
cargo audit --version
cargo audit --deny warnings
cargo test --locked
