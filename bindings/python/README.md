# geoda (Python)

Binding de Python de geoda: PyO3 sobre `geoda-core`, `geoda-algo` y `geoda-stats`, construido con
maturin. Estadistica sobre columnas, con el nucleo en Rust.

## Instalar

Todavia no esta en PyPI. Se construye una wheel local y se instala con `pip`:

```bash
pip install maturin
bindings/python/build_wheel.sh /tmp/wheelhouse     # wheel de Linux, sin rutas de tu maquina
python -m venv /tmp/venv
/tmp/venv/bin/pip install /tmp/wheelhouse/geoda-*.whl pyarrow
/tmp/venv/bin/python examples/estadisticas.py      # resumen de una tabla, con comprobaciones
```

`pyarrow` es opcional: solo hace falta para pasar columnas Arrow. Con listas no se necesita.
Usa `build_wheel.sh` y no `maturin build` a secas: el script reescribe las rutas absolutas que
el compilador guarda en el binario.

## API

Todas las funciones numericas aceptan una **lista** (`None` = nulo) o cualquier objeto con el
protocolo Arrow PyCapsule (`__arrow_c_array__`: `pyarrow`, `polars`, `pandas` con Arrow...).
Los nulos se ignoran. Un `NaN` en los datos da `NaN` en el resultado, nunca se oculta.

| Funcion | Devuelve |
|---|---|
| `count(x)`, `sum(x)`, `min(x)`, `max(x)`, `mean(x)` | `count`: `int`; `sum`/`min`/`max`: `int` si `x` es una columna Arrow entera, `float` si no; `mean`: `float` |
| `var(x, ddof=1)`, `std(x, ddof=1)` | `float` |
| `median(x)`, `percentile(x, p)` | `float` (`p` en `[0, 100]`, interpolacion lineal tipo 7) |
| `corr(x, y)` | `float` en `[-1, 1]` (Pearson; las filas con nulo en cualquiera se descartan) |
| `histogram(x, bins, range=None)` | `dict` con `edges`, `counts`, `below`, `above`, `nulls`, `nans` |
| `group_by(claves).sum/count/min/max/mean/var/std(valores)` | `dict` con `keys` y `values` (listas; `None` es el grupo de claves nulas) |
| `edit_distance(a, b)` | `int` con dos `str`; si no, lista con la distancia de cada fila (`None` si alguna es nula). Acepta `str`, listas de `str`/`None` o columnas Arrow de texto; las dos miden lo mismo o una mide 1. Cuenta caracteres Unicode |
| `dedupe(x, max_distance=1)` | `dict` con `ids` (grupo de cada fila, desde 0; `None` en las nulas) y `count`. Dos cadenas estan cerca si su distancia de edicion es `<= max_distance`; un grupo es una componente conexa de «estar cerca» (si a~b y b~c van juntos aunque a y c no lo esten) |
| `kmeans(columnas, k, seed=0, max_iter=100, tol=1e-4)` | `dict` con `labels`, `centroids` (lista de `k` listas, una por grupo), `inertia` e `iterations`. `columnas` es una lista de columnas (una por variable); sin `None` ni `NaN`. La semilla fija el resultado |
| `hierarchical(columnas)` | `dict` con `n`, `left`, `right`, `height` y `size`: el dendrograma de enlace simple (hasta 5000 filas) al estilo de `scipy` |
| `cut_tree(dendrograma, k)` | lista con el grupo (`0..k`) de cada fila |
| `DecisionTree(max_depth=8, min_samples_leaf=1)` | `.fit(columnas, etiquetas)` entrena (CART con Gini, solo variables numericas) y devuelve el propio arbol; `.predict(columnas)` da la clase de cada fila; `.n_nodes` y `.depth`. Etiquetas enteros `>= 0`. Determinista |
| `frequent_itemsets(transacciones, min_support)` | `dict` con `itemsets` (listas de items, por tamano y luego en orden lexicografico), `support` (transacciones que contienen a cada uno) y `transactions`. `transacciones` es una lista de listas de enteros `>= 0`; `min_support` es un conteo absoluto |
| `association_rules(transacciones, min_support, min_confidence)` | lista de `dict` con `antecedent`, `consequent` (listas de items), `support`, `confidence` y `lift`; `min_confidence` en `(0, 1]` |
| `matmul(a, b)` | lista de filas del producto. Una **matriz matematica** es una lista de filas (como NumPy); una tabla de datos (`kmeans`, `DecisionTree`...) sigue siendo una lista de columnas |
| `solve(a, b)` | `x` de `a x = b` por LU con pivoteo parcial; `ValueError` si `a` es singular o no es cuadrada |
| `det(a)` | determinante (`0.0` si es singular, sin error) |
| `inv(a)` | lista de filas de la inversa; `ValueError` si es singular |
| `qr(a)` | `dict` con `q` y `r` (listas de filas): `a = q r` por Householder, con al menos tantas filas como columnas |
| `svd(a)` | `dict` con `u`, `s` y `vt`: SVD delgada con valores de mayor a menor y signo fijo (la mayor entrada de cada columna de `V` es positiva) |
| `lstsq(a, b)` | coeficientes `x` que minimizan `\|a x - b\|`; `ValueError` si las columnas son dependientes |
| `LinearRegression(fit_intercept=True)` | `.fit(columnas, y)` ajusta por minimos cuadrados (QR) y devuelve el modelo; `.predict(columnas)`; `.coef_`, `.intercept_` y `.r2_`. `ValueError` si las variables son colineales |
| `PCA(n_components)` | `.fit(columnas)` centra (sin escalar) y usa la SVD; `.transform(columnas)` da una lista de filas; `.components_`, `.explained_variance_`, `.explained_variance_ratio_` y `.mean_`. Signo determinista |
| `lis(x)` | `dict` con `length` e `indices`: la subsecuencia estrictamente creciente mas larga. `None` o `NaN` lanzan `ValueError` |
| `huffman_encode(datos)` | `dict` con `bits` (`bytes`, empaquetados desde el bit mas alto), `bit_len` y `frequencies` (256 `int`). `datos` es `bytes`, `bytearray` o `str` (UTF-8) |
| `huffman_decode(bits, bit_len, frequencies)` | `bytes`; la tabla se reconstruye de las frecuencias. Bits que no se pueden decodificar lanzan `ValueError` |

- **`workers` (por defecto 1)** en `sum`, `min`, `max`, `mean`, `var`, `std`, `corr`, `histogram` y
  `group_by(...).<agg>` reparte los bloques de 1024 valores entre ese numero de hilos. El
  resultado es idéntico bit a bit con cualquier valor; menos de 1 lanza `ValueError`. Una columna
  de un solo bloque no lanza hilos.
- **`ddof` por defecto es 1** (varianza muestral, como R y pandas). **NumPy usa 0**:
  `np.var(a)` y `geoda.var(a)` no coinciden; usa `geoda.var(a, ddof=0)` para igualarlos.
- El histograma usa cubos `[a, b)` salvo el ultimo, cerrado. Lo que cae fuera de `range` se
  cuenta en `below` y `above`, no se descarta.
- Un tipo no soportado (texto, `decimal128`, listas anidadas) lanza `TypeError`; un argumento
  invalido (`ddof >= n`, sin valores validos, largos distintos) lanza `ValueError`.

## Limites conocidos

- **Solo Linux (x86_64)**: la wheel se construye en tu maquina y no se garantiza que sirva en otra distribucion; wheels `manylinux`, macOS, Windows y PyPI quedan para mas adelante.
- **Los datos se copian** al pasar una columna Arrow (no hay cero copia).
- Las **listas** de Python se convierten a `float`: `geoda.sum([1, 2, 3])` da `6.0`, y los
  enteros de magnitud mayor a 2^53 pierden precision. Con una columna Arrow entera, `sum`,
  `min` y `max` dan `int`.
- `histogram` y `group_by` devuelven listas dentro de un `dict`, no una tabla Arrow.
- `ChunkedArray` (`__arrow_c_stream__`) no se acepta; usa `combine_chunks()`.

## Desarrollo

```bash
cargo build -p geoda-capi                 # libgeoda, para el runner de spec
cd bindings/python
uv venv .venv && uv pip install maturin pyarrow numpy
VIRTUAL_ENV=$PWD/.venv .venv/bin/maturin develop
.venv/bin/python -c "import geoda; print(geoda.version())"
.venv/bin/python -m unittest discover -s tests -v
```

## Tests

- `tests/test_spec.py`: runner de `spec/` por `ctypes` sobre `libgeoda` (solo stdlib).
  Hace lo mismo que `crates/capi/tests/spec.rs`.
- `tests/test_interop_pyarrow.py`: pyarrow como oraculo de Arrow, en ambos sentidos.
  Se salta si `pyarrow` no esta instalado; en CI debe correr.
- `tests/test_model.py`: `LinearRegression`, `PCA` y los casos de `spec/cases/model/`.
- `tests/test_linalg.py`: `matmul`, `solve`, `det`, `inv` y los casos de `spec/cases/linalg/`.
- `tests/test_apriori.py`: `frequent_itemsets`, `association_rules` y los casos de `spec/cases/apriori/`.
- `tests/test_tree.py`: `DecisionTree` y los casos de `spec/cases/tree/`.
- `tests/test_cluster.py`: `kmeans`, `hierarchical`, `cut_tree` y los casos de `spec/cases/cluster/`.
- `tests/test_text.py`: la fase 5 (distancia de edicion, LIS y Huffman) y los casos de
  `spec/cases/text/` que se pueden ejecutar por el binding.
- `tests/test_arrow_input.py`, `test_stats.py`, `test_tables.py`: la API de arriba, con listas y
  con columnas Arrow (los de Arrow necesitan `pyarrow` y `numpy`).
- `tests/capi.py`: declaraciones `ctypes` de la C-API y de las structs de Arrow.
  `GEODA_LIB` apunta a otra `libgeoda` si no esta en `target/{debug,release}`.
