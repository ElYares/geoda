# spec/

La verdad de geoda vive aqui: casos de prueba en JSON, sin dependencias de ningun
lenguaje. **Si un binding discrepa de un caso, el binding esta mal.**

`spec_version` actual: **1**. Un runner rechaza un caso con otra version.

## Layout

```
spec/
├── README.md              este archivo
└── cases/<grupo>/<caso>.json
```

Cada runner recorre `cases/**/*.json`. Hay dos, que deben
coincidir: uno en Rust (`crates/capi/tests/spec.rs`, corre con
`cargo test -p geoda-capi spec`) y otro en Python (`bindings/python/tests/test_spec.py`).

## Formato de un caso

```json
{
  "spec_version": 1,
  "id": "column/slice_int32_nulls",
  "description": "un corte recuenta sus propios nulos",
  "input":  { "dtype": "int32", "values": [10, null, 30, 40, null] },
  "op":     "slice",
  "args":   { "offset": 1, "len": 3 },
  "expect": { "dtype": "int32", "values": [null, 30, 40], "null_count": 1, "len": 3 }
}
```

| Campo | Obligatorio | Significado |
|---|---|---|
| `spec_version` | si | version del formato |
| `id` | si | `grupo/nombre`, igual a la ruta del archivo sin `.json` |
| `description` | si | que comportamiento fija el caso y por que |
| `input` | si (salvo `graph_*`) | la columna de entrada: `dtype` y `values` |
| `graph` | `graph_*` | el grafo de entrada en lugar de `input` (ver «Grafos») |
| `op` | si | la operacion (ver abajo) |
| `input2` | `corr`, `group_by` | la segunda columna, con el mismo formato que `input` (en `group_by` son los valores; `input` son las claves) |
| `args` | segun `op` | parametros de la operacion; `workers` (opcional, por defecto 1) es el numero de hilos de `sum`, `min`, `max`, `mean`, `var`, `std`, `corr`, `histogram` y `group_by`: el resultado no depende de el y `0` es `INVALID_ARGUMENT` |
| `expect` | si | el resultado esperado, o un error |

### Tipos (`dtype`)

`bool`, `int32`, `int64`, `float64`, `utf8`. `values` es una lista; `null` es un nulo.
Los enteros y los booleanos son numeros y `true`/`false` de JSON; el texto, cadenas.

Valores especiales de `float64`, como cadenas: `"NaN"`, `"Infinity"`, `"-Infinity"`,
`"-0.0"`. La comparacion es **exacta** (bit a bit, y `NaN` igual a `NaN`): `-0.0` y
`0.0` son distintos. No hay tolerancias en la version 1.

`decimal128` es un tipo de **prueba negativa**: el runner lo presenta a la capi como
el formato Arrow `d:10,2`, que geoda aun no soporta (`UNSUPPORTED`).

### Operaciones (`op`)

Una operacion nueva se anade aqui **antes** de implementarla. Una `op` que el runner
no conozca **hace fallar el caso**, no lo salta.

| `op` | `args` | Que hace | Que se compara |
|---|---|---|---|
| `roundtrip` | — | importa la columna por la capi y la exporta de vuelta | `expect` completo sobre lo exportado |
| `len` | — | pide la longitud al handle | `expect.len` |
| `null_count` | — | pide la cuenta de nulos al handle | `expect.null_count` |
| `slice` | `offset`, `len` | corta `[offset, offset + len)` y exporta el corte | `expect` completo sobre el corte |
| `median` | — | mediana de la columna (`geoda_column_median`) | `expect.value` |
| `percentile` | `p` | percentil `p` en `[0, 100]`, tipo 7 (`geoda_column_percentile`) | `expect.value` |
| `count` | — | valores no nulos (`geoda_column_count`) | `expect.value` (entero) |
| `sum` | — | suma de los no nulos (`geoda_column_sum_i64` o `_f64`) | `expect.value` |
| `min` | — | minimo de los no nulos (`geoda_column_min_i64` o `_f64`) | `expect.value` |
| `max` | — | maximo de los no nulos (`geoda_column_max_i64` o `_f64`) | `expect.value` |
| `mean` | — | media de los no nulos, siempre `float64` (`geoda_column_mean`) | `expect.value` |
| `var` | `ddof` | varianza con `ddof` grados de libertad restados (`geoda_column_var`) | `expect.value` |
| `std` | `ddof` | desviacion estandar, `sqrt(var)` (`geoda_column_std`) | `expect.value` |
| `corr` | — | correlacion de Pearson entre `input` e `input2` (`geoda_column_corr`) | `expect.value` |
| `histogram` | `n_bins`, `range` (opcional) | histograma de ancho fijo (`geoda_column_histogram`) | `expect.columns` |
| `group_by` | `agg`, `ddof` (solo `var`/`std`) | agregado de `input2` por los grupos de `input` (`geoda_column_group_by`) | `expect.columns` |
| `edit_distance` | — | distancia de edicion fila a fila entre `input` e `input2` (`geoda_column_edit_distance`) | `expect.columns` |
| `dedupe` | `max_distance` | agrupa las filas de `input` (`utf8`) que estan cerca (`geoda_column_dedupe`) | `expect.columns` y `expect.count` |
| `lis` | — | posiciones de la subsecuencia creciente mas larga (`geoda_column_lis`) | `expect.columns` |
| `huffman_encode` | — | codifica los bytes de `input` (`geoda_huffman_encode`) | `expect.columns` |
| `huffman_decode` | — | decodifica `input2` con las frecuencias de `input` (`geoda_huffman_decode`) | `expect.columns` |
| `kmeans` | `k`, `seed`, `max_iter`, `tol` | k-means sobre `inputs` (`geoda_kmeans`) | `expect.columns`, `expect.inertia` y `expect.iterations` |
| `hierarchical` | — | dendrograma de enlace simple sobre `inputs` (`geoda_hierarchical`) | `expect.columns` |
| `dendrogram_cut` | `n`, `k` | corta un dendrograma dado por `input` (left) e `input2` (right) (`geoda_dendrogram_cut`) | `expect.columns` |
| `tree_fit_predict` | `max_depth`, `min_samples_leaf` | entrena un arbol con `inputs` y `labels` y predice `predict_inputs` (`geoda_tree_fit` y `geoda_tree_predict`) | `expect.columns` y `expect.nodes` |
| `apriori_itemsets` | `min_support` | conjuntos frecuentes de las transacciones `input` (transaccion) e `input2` (item) (`geoda_apriori_itemsets`) | `expect.columns` |
| `apriori_rules` | `min_support`, `min_confidence` | reglas de asociacion de esas transacciones (`geoda_apriori_rules`) | `expect.columns` |
| `matmul` | — | producto `inputs * inputs2` (`geoda_matmul`) | `expect.columns` |
| `lu_solve` | — | `inputs x = input` por LU (`geoda_lu_solve`) | `expect.columns` |
| `det` | — | determinante de `inputs` (`geoda_det`) | `expect.value` |
| `inverse` | — | inversa de `inputs` (`geoda_inverse`) | `expect.columns` |
| `qr` | — | factorizacion QR delgada de `inputs` (`geoda_qr`) | `expect.columns` |
| `svd` | — | SVD delgada de `inputs` (`geoda_svd`) | `expect.columns` |
| `lstsq` | — | minimos cuadrados `inputs x ~ input` (`geoda_lstsq`) | `expect.columns` y `expect.residual_norm` |
| `regression_fit_predict` | `fit_intercept` | ajusta una regresion lineal con `inputs` e `y` y predice `predict_inputs` (`geoda_regression_*`) | `expect.columns`, `expect.intercept` y `expect.r_squared` |
| `pca_fit_transform` | `n_components` | ajusta un PCA con `inputs` y proyecta `predict_inputs` (`geoda_pca_*`) | `expect.columns` |
| `graph_bfs` | `source` | recorrido en anchura (`geoda_graph_bfs`) | `expect.columns` |
| `graph_components` | — | componentes conexas (`geoda_graph_components`) | `expect.columns` y `expect.count` |
| `graph_topological_order` | — | orden topologico (`geoda_graph_topological_order`) | `expect.columns` |
| `graph_dijkstra` | `source` | caminos minimos (`geoda_graph_dijkstra`) | `expect.columns` |
| `graph_mst` | — | bosque de expansion minimo (`geoda_graph_mst`) | `expect.columns`, `expect.total` y `expect.components` |

`median` y `percentile` aceptan `int32`, `int64` y `float64`, ignoran los nulos y
devuelven un `float64`. Un `NaN` en la columna da `NaN` y no es un error.

#### Estadisticos escalares (fase 2)

`count`, `sum`, `min`, `max`, `mean`, `var`, `std` y `corr` aceptan `int32`, `int64` y
`float64` (`bool` y `utf8` dan `UNSUPPORTED`) e ignoran los nulos. Un `NaN` en la columna da
`NaN` en todos menos en `count`, y no es un error (S5). Los valores salen asi:

- `count`: un entero (`i64`); no falla nunca con una columna numerica.
- `sum`, `min`, `max`: **conservan el tipo de la columna**. El runner llama a la variante
  `_i64` si `input.dtype` es `int32` o `int64`, y a la `_f64` si es `float64`; el resultado
  entero se escribe en `expect.value` como entero de JSON y se compara exacto. La variante
  que no corresponde al tipo de la columna es `TYPE_MISMATCH`. Una suma entera que no cabe
  en `i64` es `OUT_OF_BOUNDS`. `min`/`max` sin valores validos son `INVALID_ARGUMENT`; `sum`
  sin valores validos es 0.
- `mean`, `var`, `std`, `corr`: siempre `float64`. `mean` sin valores validos es
  `INVALID_ARGUMENT`.
- `var` y `std`: `ddof` es **obligatorio** en la capi (S2; el valor por defecto `1` es cosa del
  binding de Python). `ddof >= n` o sin valores validos es `INVALID_ARGUMENT`.
- `corr`: las dos columnas deben medir lo mismo (`LENGTH_MISMATCH`); las filas con nulo en
  cualquiera de las dos se descartan en ambas (S7); una columna constante (varianza cero)
  es `INVALID_ARGUMENT`. El resultado se acota a `[-1, 1]`. Como la capi no tiene
  tolerancias, un caso de `corr` usa datos cuyo resultado es exacto (p. ej. `1`, `-1`).

#### Resultados de varias columnas (fase 2)

`histogram` y `group_by` devuelven **varias columnas nuevas** (handles), no un escalar. La capi
las entrega por varios `out` en una sola llamada; cada una se exporta por Arrow y se libera con
`geoda_column_free`. Si la llamada falla, **todos** los `out` quedan en NULL.

- `histogram` (`args.n_bins`; `args.range` = `[lo, hi]` o ausente para `[min, max]`; la capi recibe `has_range`, `lo` y `hi` por valor):
  1. `edges`: `float64`, `n_bins + 1` bordes. El cubo `i` es `[edges[i], edges[i+1])` y el ultimo es
     cerrado.
  2. `counts`: `int64`, `n_bins` conteos.
  3. `outside`: `int64`, 4 valores `[below, above, nulls, nans]`. Lo que no cae en ningun cubo se
     cuenta aparte, nunca se pierde (S9).
  Con `min == max` el rango se ensancha a `[v - 0.5, v + 0.5]`; `n_bins = 0`, un rango invertido o no
  finito, o ningun valor valido sin rango explicito son `INVALID_ARGUMENT`.
- `group_by` (`args.agg` = `count`, `sum`, `min`, `max`, `mean`, `var` o `std`; `args.ddof` para
  `var` y `std`). Las filas de `input` con la misma clave forman un grupo, y el agregado se calcula
  sobre las filas correspondientes de `input2`:
  1. `keys`: del **mismo `dtype` que `input`**, una fila por grupo, en orden de primera aparicion. Las
     claves nulas forman su propio grupo, que sale como nulo (S8). `NaN` es una sola clave y `-0.0` y
     `0.0` son claves distintas.
  2. `values`: `int64` para `count` y para `sum`/`min`/`max` sobre enteros; `float64` en los demas
     casos. Reglas de nulos, `NaN` y errores, las del escalar correspondiente en cada grupo; el error
     de un grupo aborta la llamada. Largos distintos son `LENGTH_MISMATCH`; `input2` `bool` o `utf8`,
     `UNSUPPORTED`.

#### Grafos (fase 4)

Un caso de grafo lleva `graph` en lugar de `input`:

```json
"graph": { "n": 5, "directed": true, "edges": [[0, 1], [0, 2]], "weights": [4.0, 1.0] }
```

`weights` es opcional (sin el, el grafo no tiene pesos); un peso usa los mismos valores
especiales que `float64`. El runner entrega las aristas a `geoda_graph_new` como dos columnas
`int64` (`from`, `to`) y los pesos como una `float64`, y libera el grafo con `geoda_graph_free`.
Un error al construir (`OUT_OF_BOUNDS` para un nodo negativo o `>= n`, `INVALID_ARGUMENT` para un
nulo, `LENGTH_MISMATCH` para pesos de otro largo) se comprueba con `expect.error`, igual que uno de
la consulta. Las columnas de resultado, en este orden:

- `graph_bfs`: `order` (los alcanzables en orden de visita), `distance` y `parent` (una fila por
  nodo, nulo si no es alcanzable; `parent` es nulo tambien en el origen), todas `int64`.
- `graph_components`: `ids` (`int64`, el id de cada nodo, desde 0 por el primer nodo de cada una);
  `expect.count` es cuantas hay.
- `graph_topological_order`: `order` (`int64`); un ciclo, un bucle o un grafo no dirigido es
  `INVALID_ARGUMENT`.
- `graph_dijkstra`: `distance` (`float64`, `Infinity` si no es alcanzable) y `parent` (`int64`).
  Sin pesos es `TYPE_MISMATCH`; un peso negativo o `NaN`, `INVALID_ARGUMENT`.
- `graph_mst`: `edges` (`int64`, posiciones en la lista de aristas de las elegidas, de menor a
  mayor peso; con el mismo peso gana la primera); `expect.total` es la suma de sus pesos
  (`float64`, en ese orden) y `expect.components` cuantas componentes tiene el grafo. Sin pesos es
  `TYPE_MISMATCH`; dirigido, `INVALID_ARGUMENT`.

`expect.count`, `total`, `components`, `nodes`, `inertia` e `iterations` se comparan exactos (el flotante bit a bit) salvo que el caso lleve `expect.tolerance`.

#### Texto y similitud (fase 5)

- `edit_distance`: `input` e `input2` son `utf8`; miden lo mismo, o una de ellas mide 1 y se
  compara con todas las filas de la otra. Una fila nula en cualquiera da nulo. Sale una columna
  `int64`. Cuenta caracteres Unicode, no bytes. Largos distintos sin una de 1 es
  `LENGTH_MISMATCH`; una entrada que no es `utf8`, `TYPE_MISMATCH`.
- `dedupe`: `input` es `utf8`; `args.max_distance` (>= 0) es la distancia de edicion maxima para estar «cerca». Un
  grupo es una componente conexa de «estar cerca» (si `a~b` y `b~c`, van juntos aunque `a` y `c` no
  lo esten). Sale una columna `int64` con el grupo de cada fila (desde 0, por primera aparicion;
  nulo en las filas nulas) y `expect.count` es cuantos grupos hay. Otro tipo es `TYPE_MISMATCH`.
- `lis`: `int32`, `int64` o `float64` sin nulos ni `NaN`. Sale una columna `int64` con las
  posiciones de una subsecuencia **estrictamente** creciente mas larga, de menor a mayor. Un nulo
  o un `NaN` es `INVALID_ARGUMENT`; `bool` y `utf8`, `UNSUPPORTED`.
- `huffman_encode`: `input` es `int32` con bytes (`0..=255`, sin nulos). Salen dos columnas: `bits`
  (`bool`, el primer bit es el de la raiz del arbol) y `frequencies` (`int64`, 256 filas). El
  desempate es fijo: menor frecuencia y, a igualdad, el simbolo mas pequeno; el menor va a la
  izquierda (`0`). Un solo simbolo distinto recibe el codigo `0`. Un valor fuera de `0..=255` o un
  nulo es `INVALID_ARGUMENT`; otro tipo, `TYPE_MISMATCH`.
- `huffman_decode`: `input` son las frecuencias (`int64`, 256 no negativas) e `input2` los bits
  (`bool`, sin nulos). Sale una columna `int32` con los bytes. Bits que terminan a media palabra,
  frecuencias que no son 256 o un nulo son `INVALID_ARGUMENT`.

#### Mineria (fase 6)

Una matriz de datos es una **lista de columnas** (una por variable): un caso de `kmeans` o
`hierarchical` lleva `inputs` (lista de columnas, cada una con `dtype` y `values`) en lugar de
`input`. El runner las importa y entrega a la capi un arreglo de handles. Las columnas son
`int32`, `int64` o `float64`, sin nulos ni `NaN` (`INVALID_ARGUMENT`); largos distintos son
`LENGTH_MISMATCH`; una lista vacia o una columna de texto, `INVALID_ARGUMENT`.

- `kmeans` (`args.k`, `args.seed` obligatorios; `args.max_iter` por defecto 100 y `args.tol` por
  defecto `1e-4`): sale `labels` (`int64`, el grupo de cada fila) y `centroids` (`float64`, `k * d`
  valores **por filas**: el centroide `c` ocupa `c * d .. (c + 1) * d`); `expect.inertia` (`float64`,
  bit a bit) y `expect.iterations` (entero). La semilla fija el resultado; los casos usan datos
  cuyas medias son exactas. `k = 0`, `k` mayor que las filas o `max_iter = 0` son `INVALID_ARGUMENT`.
- `hierarchical`: salen `left`, `right`, `height` y `size` (`n - 1` filas cada una; vacias con una
  sola fila). Los grupos `0..n` son las filas y la union `s` crea el grupo `n + s`. Mas de 5000 filas
  es `INVALID_ARGUMENT`.
- `apriori_itemsets` / `apriori_rules`: las transacciones van en **formato largo**: `input` es
  `transaction` e `input2` es `item` (dos columnas `int64` del mismo largo, ids `>= 0`, sin nulos; un
  item repetido dentro de una transaccion cuenta una vez). `args.min_support` es un conteo absoluto
  (>= 1). `apriori_itemsets` saca `set` (id del conjunto, una fila por item), `item` y `support`
  (una fila por conjunto); los conjuntos van por tamano y luego en orden lexicografico, con los items
  de menor a mayor. `apriori_rules` (`args.min_confidence` en `(0, 1]`) saca `antecedent` y
  `consequent` (ids de esos mismos conjuntos), `support` (`int64`), `confidence` y `lift` (`float64`,
  bit a bit), por conjunto frecuente y, dentro de cada uno, por la mascara de bits del antecedente.
  `min_support = 0`, ids negativos o nulos y `min_confidence` fuera de `(0, 1]` son
  `INVALID_ARGUMENT`; largos distintos, `LENGTH_MISMATCH`.
- `tree_fit_predict` (`args.max_depth` por defecto 8, `args.min_samples_leaf` por defecto 1): `inputs`
  son las variables de entrenamiento, `labels` una columna `int64` (>= 0, sin nulos; puede ser dispersa)
  y `predict_inputs` (opcional, por defecto `inputs`) las filas a predecir. Sale una columna `int64`
  con la clase de cada fila y `expect.nodes` es la cantidad de nodos del arbol (hojas incluidas). El
  arbol es determinista (desempates: menor impureza, menor variable, menor umbral; en una hoja con
  clases empatadas, la menor). Etiquetas negativas o nulas, `min_samples_leaf = 0` o sin filas son
  `INVALID_ARGUMENT`; etiquetas que no son `int64`, `TYPE_MISMATCH`; etiquetas de otro largo,
  `LENGTH_MISMATCH`; predecir con otro numero de variables, `LENGTH_MISMATCH`.
- `dendrogram_cut` (`args.n`, `args.k`): `input` es `left` e `input2` es `right` (`int64`, `n - 1`
  filas); sale `labels` (`int64`, `0..k`, numerados por la primera fila de cada grupo). `k` fuera de
  `1..=n`, un largo que no es `n - 1` o una union con un grupo que aun no existe son
  `INVALID_ARGUMENT`; un negativo, `OUT_OF_BOUNDS`; otro tipo, `TYPE_MISMATCH`.

#### Algebra lineal (fase 7)

Una **matriz matematica** de entrada es, como una tabla de datos, una lista de columnas (`inputs`: la
columna `j` de la lista es la columna `j` de la matriz); `matmul` lleva ademas `inputs2` (la matriz
`B`) y `lu_solve` una columna `input` (el lado derecho `b`). Sin nulos ni valores no finitos
(`INVALID_ARGUMENT`); una lista vacia, `INVALID_ARGUMENT`. Una matriz de **salida** sale como **una**
columna `float64` plana **por filas** (decision 014, N2), de la que quien llama conoce las dimensiones:

- `matmul`: `filas(A) * columnas(B)` valores. `A` y `B` que no encajan, `LENGTH_MISMATCH`.
- `lu_solve`: `x`. `A` singular, `INVALID_ARGUMENT`; `A` no cuadrada o `b` de otro largo,
  `LENGTH_MISMATCH`.
- `det`: un escalar (`expect.value`). Una matriz singular tiene determinante `0` y **no** es un error;
  una no cuadrada, `LENGTH_MISMATCH`.
- `inverse`: `n * n` valores. Singular, `INVALID_ARGUMENT`.
- `qr`: dos columnas planas, `q` (`m * n`, columnas ortonormales) y `r` (`n * n`, triangular superior).
  Hace falta `m >= n` (`INVALID_ARGUMENT`). El signo es el de las reflexiones de Householder: la diagonal
  de `R` suele salir negativa.
- `svd`: tres columnas, `u` (`m * k`, plana por filas), `s` (los `k = min(m, n)` valores singulares,
  de mayor a menor) y `vt` (`k * n`, plana por filas). Signo fijo (N4): la mayor entrada de cada
  columna de `V` es positiva. Acepta matrices anchas.
- `regression_fit_predict` (`args.fit_intercept`, por defecto `true`): `inputs` son las variables, `y` la
  columna a explicar (`float64`, `int32` o `int64`, sin nulos) y `predict_inputs` (opcional, por defecto
  `inputs`) las filas a predecir. Salen `coefficients` (uno por variable) y `predictions`; `expect.intercept`
  (`0` sin intercepto) y `expect.r_squared` (respecto a la media de `y`). Menos filas que coeficientes,
  variables colineales (`rango incompleto`), nulos o `NaN`, `INVALID_ARGUMENT`; `y` de otro largo o
  predecir con otro numero de variables, `LENGTH_MISMATCH`.
- `pca_fit_transform` (`args.n_components`): `inputs` son las variables y `predict_inputs` (opcional) las
  filas a proyectar. Salen cinco columnas: `components` (`k * d` por filas, un componente por fila),
  `explained_variance` (`s^2 / (n - 1)`), `explained_variance_ratio`, `mean` y `transformed`
  (`filas * k` por filas). Signo fijo (N4). `n_components` fuera de `1..=min(filas, d)`, menos de 2
  filas, nulos o `NaN`, `INVALID_ARGUMENT`; transformar con otro numero de variables, `LENGTH_MISMATCH`.
- `lstsq`: una columna `x` (los `n` coeficientes) y `expect.residual_norm` (la norma de `A x - b`).
  `m < n` o columnas dependientes, `INVALID_ARGUMENT`; `b` de otro largo, `LENGTH_MISMATCH`.

**Tolerancia.** `expect.tolerance` (opcional, un numero finito `>= 0`) es el error absoluto maximo
con el que se comparan los valores `float64` de `expect.columns`, `expect.value` y los escalares con
nombre del caso. Sin ella la comparacion sigue siendo exacta (bit a bit); un `NaN` solo iguala a un
`NaN` y un infinito solo a si mismo, tambien con tolerancia. Los casos de resultado exacto (enteros
pequenos) no la llevan.

### `expect`

Se comprueban solo las claves presentes:

- `dtype`, `values`: sobre la columna que **sale** de la capi (exportada).
- `len`, `null_count`: sobre el handle resultante.
- `value`: el numero que devuelve la operacion. Un `float64` se compara bit a bit y `NaN` es igual a `NaN`, como el resto de los flotantes; se escribe con los mismos valores especiales. Un resultado entero (`count`, y `sum`/`min`/`max` sobre enteros) se escribe como entero de JSON y se compara exacto.
- `columns`: en `histogram`, `group_by`, las operaciones de texto y las de grafo. Una lista, en el orden de arriba, de objetos con las
  mismas claves `dtype` y `values` de una columna exportada; se comprueba cada una contra la columna
  que **sale** de la capi, con la misma comparacion exacta. Tiene que haber tantas entradas como
  columnas devuelve la operacion.
- `error`: la operacion debe fallar con esa categoria; si tiene exito, o falla con otra,
  el caso falla. Las categorias son el sufijo de `GEODA_ERR_*` en `geoda.h`:
  `INVALID_ARGUMENT`, `TYPE_MISMATCH`, `LENGTH_MISMATCH`, `OUT_OF_BOUNDS`,
  `UNSUPPORTED`, `ALLOC`, `FFI`, `PANIC`, `INTERNAL`.

`error` no se mezcla con las demas claves.

## Reglas

- Los casos no llevan datos grandes; lo voluminoso va a `bench/`.
- Cada funcion nueva de la C-API anade al menos un caso aqui.
- Si `spec/` crece a miles de casos, se genera con un script; no antes.
