#!/usr/bin/env bash
# Construye el binding y corre sus pruebas contra libgeoda (`cargo build -p geoda-capi`).
# La biblioteca se toma de $GEODA_LIB o, si no, de target/debug/libgeoda.so del repo.
set -euo pipefail
cd "$(dirname "$0")"
./build.sh >/dev/null
export GEODA_LIB="${GEODA_LIB:-$(cd ../.. && pwd)/target/debug/libgeoda.so}"
rm -rf build/test-classes
mkdir -p build/test-classes
javac --release 22 -Xlint:all,-restricted -Werror -cp build/geoda.jar -d build/test-classes $(find src/test -name '*.java')
java --enable-native-access=ALL-UNNAMED -cp build/geoda.jar:build/test-classes io.geoda.GeodaTest
