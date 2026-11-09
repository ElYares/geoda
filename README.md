# geoda

[![CI](https://github.com/ElYares/geoda/actions/workflows/ci.yml/badge.svg)](https://github.com/ElYares/geoda/actions/workflows/ci.yml)
[![Licencia: MIT OR Apache-2.0](https://img.shields.io/badge/licencia-MIT%20OR%20Apache--2.0-blue.svg)](#licencia)

Librería de análisis, ciencia y minería de datos escrita desde cero y pensada para escalar.
Un solo núcleo en Rust con una frontera C estable (`capi`, **ABI 1.0 congelada**: [`docs/abi.md`](docs/abi.md))
y bindings para Python, C++, Java, Node y WebAssembly.

> **Estado: versión 0.x, todavía sin paquetes publicados.** No se instala desde PyPI, npm, Maven ni crates.io;
> se construye desde el código (ver [Instalar](#instalar)). Las APIs de los bindings pueden cambiar en cada
> `0.minor`; la C-API no ([`docs/versiones.md`](docs/versiones.md)).

## Qué hay

| Área | Qué incluye |
|---|---|
| Columnas | capa columnar propia (sin dependencias), interoperable con Arrow por la C Data Interface |
| Algoritmos base | ordenación, selección lineal, heaps, hash, búsqueda binaria |
| Estadística | media, varianza estable, cuantiles, correlación, histogramas, agregados por grupo; bloques fusionables, hilos reales y columnas más grandes que la RAM |
| Grafos | BFS/DFS, componentes, orden topológico, Dijkstra, árbol de expansión mínimo |
| Texto | distancia de edición, subsecuencia creciente más larga, Huffman, deduplicación por similitud |
| Minería | k-means, agrupamiento jerárquico, árbol de decisión, reglas de asociación (Apriori) |
| Álgebra lineal | LU, QR, SVD, mínimos cuadrados, regresión lineal, PCA |

La verdad del comportamiento vive en [`spec/`](spec/README.md): casos en JSON que corren los runners de Rust y de
Python (y un subconjunto, los de JavaScript). Si un binding discrepa de un caso, el binding está mal.

## Instalar

Aún no hay paquetes publicados: se instala desde el código. Hace falta Rust 1.98 o mayor.

```bash
git clone https://github.com/ElYares/geoda.git && cd geoda
cargo build -p geoda-capi --release          # libgeoda.so, la base de todos los bindings
```

Python (3.10 o mayor, con [`uv`](https://docs.astral.sh/uv/) o `pip`):

```bash
pip install maturin
bindings/python/build_wheel.sh /tmp/wheelhouse          # wheel de Linux
python -m venv /tmp/venv && /tmp/venv/bin/pip install /tmp/wheelhouse/geoda-*.whl
```

Los demás bindings se construyen con los comandos de [Construir y probar](#construir-y-probar) y se
detallan en el README de cada uno. `release/build.sh` arma todos los paquetes en `dist/`.

## Ejemplos

Python:

```python
import geoda

geoda.mean([1.5, None, 3.5])                              # 2.5  (None = nulo, se ignora)
geoda.median([1, 2, 3, 4])                                # 2.5
geoda.edit_distance("kitten", "sitting")                  # 3
geoda.solve([[2, 1], [1, 3]], [3, 5])                     # [0.8, 1.4]
geoda.kmeans([[0, 0, 10, 10], [0, 1, 10, 11]], 2, seed=1)["labels"]   # [0, 0, 1, 1]
```

C++, Java, Node y WASM tienen la misma forma (columna, estadístico, álgebra, k-means); por ejemplo, Node:

```js
const geoda = require('@elyares/geoda');   // tras construirlo desde el código
const column = geoda.Column.fromFloat64(Float64Array.of(1.5, 99, 3.5), Uint8Array.of(1, 0, 1)); // el 99 es nulo
geoda.mean(column);                                          // 2.5
geoda.solve([[2, 1], [1, 3]], [3, 5]);                       // [0.8, 1.4]
column.free();
```

El ejemplo de cada lenguaje está en el README de su binding.

## Bindings

Todos hablan con la misma C-API ([`geoda.h`](crates/capi/include/geoda.h)); ninguno tiene lógica propia.

| Lenguaje | Carpeta | Cómo | Instalable hoy |
|---|---|---|---|
| Python | [`bindings/python`](bindings/python/README.md) | PyO3 + maturin; la API más completa | desde el código (wheel local) |
| C++ | [`bindings/cpp`](bindings/cpp/README.md) | solo cabeceras (C++17) + CMake | desde el código (CMake) |
| Java | [`bindings/java`](bindings/java/README.md) | API de memoria externa (FFM, JDK 22+), sin JNI | desde el código (`bindings/java/build.sh`) |
| Node | [`bindings/node`](bindings/node/README.md) | addon N-API en C | desde el código (`node-gyp`) |
| WebAssembly | [`bindings/wasm`](bindings/wasm/README.md) | la capi a `wasm32`, sin hilos ni WASI | desde el código (`bindings/wasm/build.sh`) |

Los de C++, Java, Node y WASM exponen la superficie mínima (columnas, estadísticos escalares, distancia de edición,
álgebra lineal básica y k-means) más `qr`, `svd`, `lstsq`, `lis`, `dedupe` e `histogram`; el de Python, todo.

## Construir y probar

```bash
cargo test --workspace --exclude geoda-py        # núcleo, capi y runner de spec en Rust
tests/c/run.sh                                   # la capi desde C: cabecera sin diff, smoke con ASan/UBSan
abi/check.sh                                     # la ABI no cambió sin avisar
cargo build -p geoda-capi                        # libgeoda.so, para los bindings
bindings/java/test.sh                            # Java
cmake -S bindings/cpp -B build/cpp -DGEODA_LIB_DIR=$PWD/target/debug && cmake --build build/cpp && ctest --test-dir build/cpp
(cd bindings/node && GEODA_LIB_DIR=$PWD/../../target/debug npx node-gyp rebuild && npm test)
bindings/wasm/build.sh && (cd bindings/wasm && npm test)
```

Python: ver [`bindings/python/README.md`](bindings/python/README.md). Benchmarks: `cargo run --release -p geoda-bench`
(`-- text`, `-- mining`, `-- linalg`, `-- graph`, `-- file`).

## Documentación

El mapa está en [`docs/README.md`](docs/README.md).

## Contribuir

Lee [`CONTRIBUTING.md`](CONTRIBUTING.md). Para reportar una vulnerabilidad: [`SECURITY.md`](SECURITY.md).

## Publicar

Nada se publica solo: [`RELEASING.md`](RELEASING.md) explica cómo construir los paquetes (`release/build.sh`), qué seguros
quitar y qué credencial pide cada registro (PyPI, npm, Maven Central, crates.io).

## Licencia

Doble licencia, a elección: [MIT](LICENSE-MIT) o [Apache-2.0](LICENSE-APACHE).
