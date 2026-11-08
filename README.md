# geoda

Librería de análisis, ciencia y minería de datos escrita desde cero y pensada para escalar.
Un solo núcleo en Rust con una frontera C estable (`capi`, **ABI 1.0 congelada**: [`docs/abi.md`](docs/abi.md))
y bindings para Python, C++, Java, Node y WebAssembly.

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

## Bindings

Todos hablan con la misma C-API ([`geoda.h`](crates/capi/include/geoda.h)); ninguno tiene lógica propia.

| Lenguaje | Carpeta | Cómo |
|---|---|---|
| Python | [`bindings/python`](bindings/python/README.md) | PyO3 + maturin; la API más completa |
| C++ | [`bindings/cpp`](bindings/cpp/README.md) | solo cabeceras (C++17) + CMake |
| Java | [`bindings/java`](bindings/java/README.md) | API de memoria externa (FFM, JDK 22+), sin JNI |
| Node | [`bindings/node`](bindings/node/README.md) | addon N-API en C |
| WebAssembly | [`bindings/wasm`](bindings/wasm/README.md) | la capi a `wasm32`, sin hilos ni WASI |

Los de C++, Java, Node y WASM exponen la superficie mínima (columnas, estadísticos escalares, distancia de edición,
álgebra lineal básica y k-means); el de Python, todo.

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

## Licencia

Doble licencia, a elección: [MIT](LICENSE-MIT) o [Apache-2.0](LICENSE-APACHE).
