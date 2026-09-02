#!/usr/bin/env bash
# Builds the DOM-backend web demo (see M0 in the "DOM render backend for the
# web" plan). Requires: `rustup target add wasm32-unknown-unknown` and
# `cargo install wasm-bindgen-cli` (version must match the `wasm-bindgen`
# dependency in Cargo.toml) done once beforehand.
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build \
  --target wasm32-unknown-unknown \
  --no-default-features \
  --features dom \
  --bin mae

wasm-bindgen \
  target/wasm32-unknown-unknown/debug/mae.wasm \
  --out-dir www/pkg \
  --target web \
  --no-typescript

echo "Built. Serve the repo root (so www/ can reach ../assets/) and open /www/, e.g.:"
echo "  python3 -m http.server 8080"
echo "  open http://localhost:8080/www/"
