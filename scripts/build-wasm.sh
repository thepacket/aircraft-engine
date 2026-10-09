#!/usr/bin/env bash
# Build the engine core to WebAssembly and generate the JS bindings used by web/.
# Requires: rustup with the wasm32-unknown-unknown target, wasm-bindgen CLI
# (same version as the wasm-bindgen crate), optionally wasm-opt (binaryen).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
export PATH="$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin:$HOME/.cargo/bin:$PATH"
OUT="$ROOT/web/src/wasm"
mkdir -p "$OUT"
cd "$ROOT"
cargo build --release --target wasm32-unknown-unknown -p engine-wasm
wasm-bindgen --target web --out-dir "$OUT" --out-name engine_wasm \
  target/wasm32-unknown-unknown/release/engine_wasm.wasm
if command -v wasm-opt >/dev/null 2>&1; then
  wasm-opt -O3 -o "$OUT/engine_wasm_bg.wasm" "$OUT/engine_wasm_bg.wasm"
fi
ls -la "$OUT"
