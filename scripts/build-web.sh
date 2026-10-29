#!/usr/bin/env bash
# Construit la version navigateur dans dist/web : deux variantes (WebGPU et WebGL2),
# la page choisit automatiquement selon le navigateur.
#
# Prérequis : rustup target add wasm32-unknown-unknown
#             cargo install wasm-bindgen-cli --version <version de wasm-bindgen dans Cargo.lock>
set -euo pipefail
cd "$(dirname "$0")/.."

OUT=dist/web
rm -rf "$OUT"
mkdir -p "$OUT"

build() {
  local backend=$1; shift
  echo "==> Build $backend"
  cargo build -p black_gem --profile wasm-release --target wasm32-unknown-unknown \
    --target-dir "target/web-$backend" "$@"
  wasm-bindgen --target web --no-typescript --out-dir "$OUT/$backend" \
    "target/web-$backend/wasm32-unknown-unknown/wasm-release/black_gem.wasm"
  if command -v wasm-opt >/dev/null; then
    wasm-opt -Os --enable-bulk-memory --enable-nontrapping-float-to-int \
      "$OUT/$backend/black_gem_bg.wasm" -o "$OUT/$backend/black_gem_bg.wasm"
  fi
}

build webgl2
build webgpu --features webgpu

cp web/index.html web/manifest.webmanifest "$OUT/"
echo "==> Prêt : $OUT (servir avec : python3 -m http.server -d $OUT 8080)"
