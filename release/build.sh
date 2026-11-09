#!/usr/bin/env bash
# Construye TODOS los paquetes de geoda en una carpeta (por defecto `dist/`). No publica nada
# (decision 016): publicar es manual y esta descrito en RELEASING.md.
#
# Hace falta en el PATH: cargo, maturin, node/npm, java/javac/jar y, para WASM, el objetivo
# wasm32-unknown-unknown (o la toolchain aislada de bindings/wasm/build.sh). Variables opcionales:
#   NODE_GYP_ARGS    p. ej. --nodedir=/usr para no bajar las cabeceras de Node
set -euo pipefail

raiz="$(cd "$(dirname "$0")/.." && pwd)"
salida="$(mkdir -p "${1:-$raiz/dist}" && cd "${1:-$raiz/dist}" && pwd)"
rm -f "$salida"/*
cd "$raiz"

echo "== libgeoda y cabeceras (C y C++) =="
cargo build -q -p geoda-capi --release
cp target/release/libgeoda.so "$salida/"
tar czf "$salida/geoda-cpp.tar.gz" crates/capi/include/geoda.h bindings/cpp/include bindings/cpp/CMakeLists.txt \
    bindings/cpp/README.md docs/abi.md abi/symbols.txt abi/VERSION

echo "== Python (wheel de esta plataforma, no manylinux) =="
bindings/python/build_wheel.sh "$salida" >/dev/null

echo "== Node (comprueba que el addon compila y empaqueta el tarball) =="
(
    cd bindings/node
    GEODA_LIB_DIR="$raiz/target/release" npx --yes node-gyp rebuild ${NODE_GYP_ARGS:-} >/dev/null
    npm pack --silent --pack-destination "$salida" >/dev/null
)

echo "== WASM =="
bindings/wasm/build.sh >/dev/null
(cd bindings/wasm && npm pack --silent --pack-destination "$salida" >/dev/null)

echo "== Java =="
bindings/java/build.sh >/dev/null
cp bindings/java/build/geoda.jar "$salida/"
cp bindings/wasm/geoda.wasm "$salida/"

(cd "$salida" && sha256sum -- * | grep -v SHA256SUMS > SHA256SUMS)
echo "== listo: $salida =="
ls -l "$salida"
