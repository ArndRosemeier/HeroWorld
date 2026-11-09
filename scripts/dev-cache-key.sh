#!/usr/bin/env bash
# The key of the prebuilt-dependency archive (.github/workflows/dev-cache.yml):
# everything that decides how the dependencies are built.
set -euo pipefail
cd "$(dirname "$0")/.."
cat Cargo.lock rust-toolchain.toml $(git ls-files '*Cargo.toml' | sort) | sha256sum | cut -c1-16
