# geoda para C++

Envoltorio C++17 **de solo cabeceras** sobre la C-API (ABI 1.x, [`docs/abi.md`](../../docs/abi.md)).
No tiene lógica propia: memoria con RAII, errores como excepciones y vectores en lugar de punteros.

```cpp
#include <geoda/geoda.hpp>

auto column = geoda::Column::from(std::vector<double>{1.5, 99.0, 3.5}, {1, 0, 1});  // el 99 es nulo
double m = geoda::mean(column);                                  // 2.5
auto x = geoda::solve({{2, 1}, {1, 3}}, {3, 5});                 // {0.8, 1.4}
auto fit = geoda::kmeans({{0, 0, 10, 10}, {0, 1, 10, 11}}, 2, /*seed=*/1);
try { geoda::median(geoda::Column::from(std::vector<double>{})); }
catch (const geoda::Error& e) { /* e.category() == "INVALID_ARGUMENT" */ }
```

## Construir y probar

```bash
cargo build -p geoda-capi
cmake -S bindings/cpp -B build/cpp -DGEODA_LIB_DIR=$PWD/target/debug
cmake --build build/cpp && ctest --test-dir build/cpp --output-on-failure
```

Compila con `g++` y `clang++` con `-Wall -Wextra -Wpedantic -Werror` y, por defecto, con ASan + UBSan
(`-DGEODA_SANITIZE=OFF` lo apaga). `cmake --install build/cpp --prefix <ruta>` copia `geoda/geoda.hpp` y
`geoda.h`; la biblioteca `libgeoda.so` sale de `cargo build -p geoda-capi --release`.

## Superficie (decisiones 015 O4 y 016 P1)

| | |
|---|---|
| `geoda::Column` | `from(vector<double/int32/int64>, validez)`, `from_bool`, `from_strings`; `size`, `null_count`, `dtype`, `validity`, `values<T>`, `optional_values<T>`, `bools`, `strings` |
| Estadísticos | `count`, `sum`, `min`, `max` (devuelven `std::variant<int64_t, double>`: entero si la columna lo es), `mean`, `var`, `std_dev` (`ddof = 1`), `median`, `percentile`, `corr` |
| Texto | `edit_distance` (dos cadenas o dos columnas) |
| Álgebra lineal | `matmul`, `solve`, `det`, `inverse`, `qr`, `svd`, `lstsq` sobre `vector<vector<double>>` **por filas** |
| Series, texto y estadística | `lis`, `dedupe`, `histogram` |
| Minería | `kmeans(variables, k, seed)` con las variables como **columnas** |
| Errores | `geoda::Error` con `code()`, `category()` y el mensaje de la capi |

Quedan solo en Python (nivel 3, decisión 016 P2): los modelos con estado (regresión, PCA, árbol), los grafos, Huffman, Apriori y `group_by`.

`workers` no se expone: siempre es 1. Para más funciones, llama a la C-API directamente (`geoda.h`) con
`geoda::Column::get()` y `release()`.
