#!/usr/bin/env bash
# Compila el binding de Java con `javac` (JDK 22 o mayor, sin jextract) y arma `build/geoda.jar`.
set -euo pipefail
cd "$(dirname "$0")"
rm -rf build/classes
mkdir -p build/classes
javac --release 22 -Xlint:all,-restricted -Werror -d build/classes $(find src/main -name '*.java')
jar --create --file build/geoda.jar -C build/classes .
echo "bindings/java/build/geoda.jar"
