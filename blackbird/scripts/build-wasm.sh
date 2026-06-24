#!/usr/bin/env bash
#
# Rebuild both shipped wasm artifacts from the Rust workspace:
#
#   web/engine/blackbird.wasm   browser build (wasm32-unknown-unknown),
#                               run by the page's Web Worker
#   component/blackbird.wasm    WIT component (wasm32-wasip2),
#                               world qubepods:blackbird/blackbird
#
# Needs the two targets once:  rustup target add wasm32-unknown-unknown wasm32-wasip2
#
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build -p blackbird-wasm --release --target wasm32-unknown-unknown
cargo build -p blackbird-component --release --target wasm32-wasip2

cp target/wasm32-unknown-unknown/release/blackbird_wasm.wasm web/engine/blackbird.wasm
cp target/wasm32-wasip2/release/blackbird_component.wasm component/blackbird.wasm

ls -la web/engine/blackbird.wasm component/blackbird.wasm
