#!/usr/bin/env bash
# Sube la version de los paquetes de geoda a la vez: Cargo.toml del workspace, Cargo.lock y los
# package.json de Node y WASM. Python la lee del Cargo.toml (dynamic). No toca abi/VERSION: la ABI
# tiene su propia version (docs/versiones.md). Uso: release/bump.sh 0.1.0
set -euo pipefail

nueva="${1:-}"
if [[ ! "$nueva" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "uso: release/bump.sh X.Y.Z" >&2
    exit 2
fi

raiz="$(cd "$(dirname "$0")/.." && pwd)"
cd "$raiz"

actual="$(sed -n '/^\[workspace.package\]/,/^\[/{s/^version = "\(.*\)"/\1/p}' Cargo.toml)"
echo "version actual: $actual -> $nueva"

sed -i "/^\[workspace.package\]/,/^\[/s/^version = \".*\"/version = \"$nueva\"/" Cargo.toml
for paquete in bindings/node/package.json bindings/wasm/package.json; do
    sed -i "0,/\"version\": \".*\"/s//\"version\": \"$nueva\"/" "$paquete"
done

# Regenera Cargo.lock con la version nueva de los crates del workspace
cargo metadata --offline --format-version 1 >/dev/null

echo "listo. Revisa con: git diff --stat"
