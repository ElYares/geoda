#!/usr/bin/env bash
# Construye la wheel de Linux de geoda sin rutas absolutas de esta maquina (HU-028).
#
# Uso: bindings/python/build_wheel.sh [directorio-de-salida]   (por defecto, dist/ junto al script)
#
# `trim-paths` de Cargo aun no es estable, asi que las rutas se reescriben con
# `--remap-path-prefix`: sin eso, los mensajes de panic y la depuracion del `.so` guardan
# /home/<usuario>/... del equipo que compilo. Requiere `maturin` en el PATH (o en el venv activo).
set -euo pipefail

aqui="$(cd "$(dirname "$0")" && pwd)"
raiz="$(cd "$aqui/../.." && pwd)"
salida="${1:-$aqui/dist}"
cargo_home="${CARGO_HOME:-$HOME/.cargo}"
sysroot="$(rustc --print sysroot)"

export RUSTFLAGS="${RUSTFLAGS:-} \
--remap-path-prefix=$raiz=geoda \
--remap-path-prefix=$cargo_home=cargo \
--remap-path-prefix=$sysroot=rustc"

cd "$aqui"
maturin build --release --out "$salida"
