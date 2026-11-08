#!/usr/bin/env bash
# Compila la C-API a WebAssembly (`wasm32-unknown-unknown`, sin hilos ni WASI) y deja `geoda.wasm` aqui.
#
# Usa el `cargo` del PATH si tiene el objetivo wasm32 (CI, `rustup target add wasm32-unknown-unknown`).
# Si no, busca una toolchain de upstream aislada en $GEODA_WASM_TOOLCHAIN o en
# ~/.local/share/geoda-wasm/toolchain (el `std` de wasm32 de upstream no es compatible con el `rustc`
# de algunas distribuciones, como Arch).
set -euo pipefail
cd "$(dirname "$0")"
raiz="$(cd ../.. && pwd)"

libdir="$(rustc --print target-libdir --target wasm32-unknown-unknown 2>/dev/null || true)"
if [ -z "$libdir" ] || [ ! -d "$libdir" ]; then
    aislada="${GEODA_WASM_TOOLCHAIN:-$HOME/.local/share/geoda-wasm/toolchain}"
    if [ -x "$aislada/bin/cargo" ]; then
        export PATH="$aislada/bin:$PATH" LD_LIBRARY_PATH="$aislada/lib:${LD_LIBRARY_PATH:-}"
    else
        echo "falta el objetivo wasm32-unknown-unknown: rustup target add wasm32-unknown-unknown," >&2
        echo "o apunta GEODA_WASM_TOOLCHAIN a una toolchain upstream con rust-std de wasm32" >&2
        exit 1
    fi
fi

CARGO_TARGET_DIR="$raiz/target/wasm" cargo build -p geoda-capi --target wasm32-unknown-unknown --release
cp "$raiz/target/wasm/wasm32-unknown-unknown/release/geoda.wasm" geoda.wasm
echo "bindings/wasm/geoda.wasm ($(wc -c < geoda.wasm) bytes)"
