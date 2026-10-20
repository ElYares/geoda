#!/usr/bin/env bash
# Verifica la C-API desde fuera de Rust:
#   1. regenera geoda.h con cbindgen y comprueba que no hay diff;
#   2. compila tests/c/smoke.c contra libgeoda y lo ejecuta;
#   3. repite con ASan + UBSan (incluye LeakSanitizer) si el compilador los trae.
set -euo pipefail

raiz="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$raiz"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

echo "== cabecera =="
cbindgen --config crates/capi/cbindgen.toml --crate geoda-capi \
    --output "$tmp/geoda.h" 2>/dev/null
diff -u crates/capi/include/geoda.h "$tmp/geoda.h" \
    || { echo "geoda.h desactualizada: regenerarla con cbindgen" >&2; exit 1; }
echo "sin diff"

echo "== smoke =="
cargo build -p geoda-capi
lib="$raiz/target/debug"
cc="${CC:-cc}"
cflags=(-std=c11 -Wall -Wextra -Werror -Icrates/capi/include)

"$cc" "${cflags[@]}" tests/c/smoke.c -L"$lib" -lgeoda -Wl,-rpath,"$lib" -o "$tmp/smoke"
"$tmp/smoke"

echo "== smoke con ASan + UBSan =="
if echo 'int main(void){return 0;}' | "$cc" -fsanitize=address,undefined -x c - -o "$tmp/probe" 2>/dev/null; then
    "$cc" "${cflags[@]}" -g -fsanitize=address,undefined \
        tests/c/smoke.c -L"$lib" -lgeoda -Wl,-rpath,"$lib" -o "$tmp/smoke_asan"
    ASAN_OPTIONS=detect_leaks=1 "$tmp/smoke_asan"
else
    echo "sin verificar: el compilador no trae ASan"
fi
