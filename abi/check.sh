#!/usr/bin/env bash
# Vigila la ABI de la C-API (decision 015, O3): la lista de simbolos `geoda_*` que exporta
# `libgeoda.so` debe ser exactamente `abi/symbols.txt`, y la version de `abi/VERSION` debe ser
# la de `ABI_MAJOR`/`ABI_MINOR` en `crates/capi/src/lib.rs`.
#
#   abi/check.sh            compara y falla si algo cambio
#   abi/check.sh --update   reescribe `abi/symbols.txt` (hacerlo a proposito: añadir = subir el menor
#                           en `lib.rs` y `abi/VERSION`; quitar o cambiar una firma = subir el mayor)
set -euo pipefail

raiz="$(cd "$(dirname "$0")/.." && pwd)"
cd "$raiz"

cargo build -q -p geoda-capi
lib="target/debug/libgeoda.so"
[ -f "$lib" ] || { echo "no encuentro $lib" >&2; exit 1; }

actual="$(mktemp)"
trap 'rm -f "$actual"' EXIT
nm -D --defined-only "$lib" | awk '$3 ~ /^geoda_/ {print $3}' | LC_ALL=C sort > "$actual"

if [ "${1:-}" = "--update" ]; then
    cp "$actual" abi/symbols.txt
    echo "abi/symbols.txt actualizado: $(wc -l < abi/symbols.txt) simbolos"
    exit 0
fi

fallo=0
if ! diff -u abi/symbols.txt "$actual"; then
    echo "" >&2
    echo "La ABI cambio. Si es a proposito: subir ABI_MINOR (o ABI_MAJOR si se quita o cambia algo)" >&2
    echo "en crates/capi/src/lib.rs y abi/VERSION, y correr abi/check.sh --update." >&2
    fallo=1
fi

mayor="$(sed -n 's/^const ABI_MAJOR: u32 = \([0-9]*\);/\1/p' crates/capi/src/lib.rs)"
menor="$(sed -n 's/^const ABI_MINOR: u32 = \([0-9]*\);/\1/p' crates/capi/src/lib.rs)"
if [ "$(cat abi/VERSION)" != "$mayor.$menor" ]; then
    echo "abi/VERSION dice $(cat abi/VERSION) pero lib.rs tiene $mayor.$menor" >&2
    fallo=1
fi

[ "$fallo" -eq 0 ] && echo "abi: $(wc -l < abi/symbols.txt) simbolos, version $mayor.$menor, sin cambios"
exit "$fallo"
