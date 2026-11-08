# geoda para Node

Binding de Node sobre la C-API (ABI 1.x, [`docs/abi.md`](../../docs/abi.md)): un **addon N-API escrito en
C** (`src/addon.c`, sin `napi-rs`) y una capa de JavaScript (`index.js`, con tipos en `index.d.ts`).

```js
const geoda = require('geoda');
const column = geoda.Column.fromFloat64(Float64Array.of(1.5, 99, 3.5), Uint8Array.of(1, 0, 1)); // el 99 es nulo
geoda.mean(column);                                          // 2.5
geoda.solve([[2, 1], [1, 3]], [3, 5]);                       // [0.8, 1.4]
geoda.kmeans([[0, 0, 10, 10], [0, 1, 10, 11]], 2, { seed: 1n }).labels;   // [0, 0, 1, 1]
try { geoda.median(geoda.Column.fromFloat64(new Float64Array(0))); }
catch (e) { e.category; /* 'INVALID_ARGUMENT', e.code === -1 */ }
column.free();                                               // idempotente; el GC libera lo que se olvide
```

## Construir y probar

```bash
cargo build -p geoda-capi
cd bindings/node
GEODA_LIB_DIR=$PWD/../../target/debug npx node-gyp rebuild    # compila build/Release/geoda_native.node
npm test                                                      # pruebas propias + el subconjunto de spec/ que soporta
npm pack                                                      # tarball: index.js, index.d.ts, binding.gyp, addon.c
```

El tarball de `npm pack` lleva el addon en C y la cabecera (`prepack` la copia), pero **no** `libgeoda`: quien lo
instale la construye con `cargo build -p geoda-capi --release` y apunta `GEODA_LIB_DIR` a su carpeta. Publicar en
npm no se hace aqui (decision 015, O7).

Con `node-gyp` instalado desde el sistema (Arch) basta `--nodedir=/usr` para no bajar las cabeceras. El addon
enlaza con `libgeoda` y deja un `rpath` a `GEODA_LIB_DIR` (por defecto `target/debug`). Hace falta Node 22 o
mayor.

## Superficie (decisión 015, O4)

| | |
|---|---|
| `Column` | `fromFloat64`, `fromInt32`, `fromBigInt64`, `fromBool`, `fromStrings` (con `validity` opcional: `Uint8Array`, 1 = válido); `length`, `nullCount`, `dtype`, `validity()`, `toFloat64Array`, `toInt32Array`, `toBigInt64Array`, `toBoolArray`, `toStrings`, `free()` |
| Estadísticos | `count`, `sum`, `min`, `max` (`bigint` en columnas enteras), `mean`, `variance`, `std` (`ddof = 1`), `median`, `percentile`, `corr` |
| Texto | `editDistance` (dos cadenas o dos columnas) |
| Álgebra lineal | `matmul`, `solve`, `det`, `inverse` sobre arreglos de **filas** |
| Minería | `kmeans(variables, k, {seed, maxIter, tol})` con las variables como **columnas** |
| Errores | `Error` con `code` y `category`; un tipo de arreglo equivocado es `TypeError` |

Los enteros de 64 bits son `BigInt`. `workers` no se expone: siempre es 1.

## Pruebas

`test/geoda.test.js` (valores de la spec) y `test/spec.test.mjs`, que ejecuta (con el runner compartido
[`bindings/spec-runner`](../spec-runner/run.mjs)) los casos de `spec/cases/` que el
binding soporta (unos 65; los demás se saltan con el motivo: operación fuera de la superficie mínima, `workers`
distinto de 1, nulos o texto dentro de una matriz). Los dos runners de Rust y Python siguen siendo los que
cubren la spec entera.
