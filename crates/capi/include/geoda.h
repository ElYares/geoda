/* geoda C-API (ABI 1.x). Generado con cbindgen: no editar a mano. */

#ifndef GEODA_H
#define GEODA_H

#include <stdarg.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>
#ifndef ARROW_C_DATA_INTERFACE
#define ARROW_C_DATA_INTERFACE

#define ARROW_FLAG_DICTIONARY_ORDERED 1
#define ARROW_FLAG_NULLABLE 2
#define ARROW_FLAG_MAP_KEYS_SORTED 4

struct ArrowSchema {
  const char* format;
  const char* name;
  const char* metadata;
  int64_t flags;
  int64_t n_children;
  struct ArrowSchema** children;
  struct ArrowSchema* dictionary;
  void (*release)(struct ArrowSchema*);
  void* private_data;
};

struct ArrowArray {
  int64_t length;
  int64_t null_count;
  int64_t offset;
  int64_t n_buffers;
  int64_t n_children;
  const void** buffers;
  struct ArrowArray** children;
  struct ArrowArray* dictionary;
  void (*release)(struct ArrowArray*);
  void* private_data;
};

#endif  // ARROW_C_DATA_INTERFACE


// Agregado de `geoda_column_group_by`: valores no nulos.
#define GEODA_AGG_COUNT 0

// Agregado de `geoda_column_group_by`: suma.
#define GEODA_AGG_SUM 1

// Agregado de `geoda_column_group_by`: minimo.
#define GEODA_AGG_MIN 2

// Agregado de `geoda_column_group_by`: maximo.
#define GEODA_AGG_MAX 3

// Agregado de `geoda_column_group_by`: media.
#define GEODA_AGG_MEAN 4

// Agregado de `geoda_column_group_by`: varianza (usa `ddof`).
#define GEODA_AGG_VAR 5

// Agregado de `geoda_column_group_by`: desviacion estandar (usa `ddof`).
#define GEODA_AGG_STD 6

// Tipo de columna devuelto por `geoda_column_dtype`: booleano.
#define GEODA_DTYPE_BOOL 0

// Tipo de columna devuelto por `geoda_column_dtype`: entero de 32 bits.
#define GEODA_DTYPE_INT32 1

// Tipo de columna devuelto por `geoda_column_dtype`: entero de 64 bits.
#define GEODA_DTYPE_INT64 2

// Tipo de columna devuelto por `geoda_column_dtype`: flotante de 64 bits.
#define GEODA_DTYPE_FLOAT64 3

// Tipo de columna devuelto por `geoda_column_dtype`: texto UTF-8.
#define GEODA_DTYPE_UTF8 4

// Exito.
#define GEODA_OK 0

// Argumento invalido (por ejemplo, un puntero NULL).
#define GEODA_ERR_INVALID_ARGUMENT -1

// El tipo recibido no es el esperado.
#define GEODA_ERR_TYPE_MISMATCH -2

// Las longitudes no coinciden.
#define GEODA_ERR_LENGTH_MISMATCH -3

// Indice u offset fuera de rango.
#define GEODA_ERR_OUT_OF_BOUNDS -4

// Operacion o tipo aun no soportado.
#define GEODA_ERR_UNSUPPORTED -5

// Fallo al reservar memoria.
#define GEODA_ERR_ALLOC -6

// Datos mal formados que llegaron por la frontera.
#define GEODA_ERR_FFI -7

// Un panic interno fue atrapado antes de cruzar la frontera.
#define GEODA_ERR_PANIC -8

// Error interno que no encaja en ninguna otra categoria.
#define GEODA_ERR_INTERNAL -9

// Columna de geoda. Opaca para C: solo se maneja por puntero.
typedef struct GeodaColumn GeodaColumn;

// Grafo de geoda con o sin pesos. Opaco para C: solo se maneja por puntero.
typedef struct GeodaGraph GeodaGraph;

// PCA ajustado. Opaco para C: solo se maneja por puntero.
typedef struct GeodaPca GeodaPca;

// Regresion lineal ajustada. Opaca para C: solo se maneja por puntero.
typedef struct GeodaRegression GeodaRegression;

// Arbol de decision entrenado. Opaco para C: solo se maneja por puntero.
typedef struct GeodaTree GeodaTree;

#ifdef __cplusplus
extern "C" {
#endif // __cplusplus

// Version de la ABI como `major << 16 | minor`. Desde la 1.0 esta congelada: añadir funciones sube
// el menor; quitar una o cambiar una firma sube el mayor (ver `docs/abi.md` y `abi/check.sh`).
uint32_t geoda_abi_version(void);

// Mensaje del ultimo error de **este hilo**, como texto UTF-8 terminado en cero.
//
// Nunca es NULL: sin errores previos devuelve la cadena vacia. El puntero vale
// hasta la siguiente llamada a la API desde el mismo hilo; copiarlo si hace falta.
const char *geoda_last_error_message(void);

// Importa una columna desde la Arrow C Data Interface (formato hoja, no struct).
//
// Copia los datos. Si devuelve `GEODA_OK`, `*out` es un handle nuevo que debe
// liberarse con `geoda_column_free` y `array->release` ya se llamo (el array
// queda consumido). `schema` solo se lee: sigue siendo del llamador.
//
// Si falla, devuelve un codigo negativo, `*out` queda en NULL y `array` **no**
// se consume: el llamador debe liberarlo.
//
// # Safety
// `schema` y `array` deben apuntar a structs validas segun la especificacion de
// Arrow, con buffers legibles completos para `offset + length`. `out` debe ser
// escribible. Ninguno puede ser NULL (si lo es, se devuelve un error).
int32_t geoda_column_from_arrow(const struct ArrowSchema *schema,
                                struct ArrowArray *array,
                                struct GeodaColumn **out);

// Exporta una columna a la Arrow C Data Interface (formato hoja, no struct).
//
// Escribe en `*out_schema` y `*out_array` structs nuevas, sin copiar los datos:
// comparten memoria con la columna y siguen vivas aunque despues se libere el
// handle. Quien las reciba debe llamar a `release` de cada una exactamente una
// vez. El campo se llama `""` y es nullable.
//
// Si falla, devuelve un codigo negativo y no escribe en ninguna de las dos.
//
// # Safety
// `column` debe ser un handle valido. `out_schema` y `out_array` deben ser
// escribibles (su contenido previo se sobrescribe sin leerse). Ninguno puede ser
// NULL (si lo es, se devuelve un error).
int32_t geoda_column_to_arrow(const struct GeodaColumn *column,
                              struct ArrowSchema *out_schema,
                              struct ArrowArray *out_array);

// Sub-columna `[offset, offset + len)` de `column`, como handle nuevo.
//
// No copia los datos (comparte buffers con `column`). Debe liberarse con
// `geoda_column_free`, en cualquier orden respecto de `column`. Si falla,
// devuelve un codigo negativo (`GEODA_ERR_OUT_OF_BOUNDS` si el rango se sale) y
// `*out` queda en NULL.
//
// # Safety
// `column` debe ser un handle valido y `out` debe ser escribible. Ninguno puede
// ser NULL (si lo es, se devuelve un error).
int32_t geoda_column_slice(const struct GeodaColumn *column,
                           size_t offset,
                           size_t len,
                           struct GeodaColumn **out);

// Mediana de una columna `Int32`, `Int64` o `Float64`, en `*out`.
//
// Los nulos se ignoran. Si algun valor es `NaN`, devuelve `GEODA_OK` con `*out = NaN`
// (no es un error). Si falla, devuelve un codigo negativo y `*out` no se escribe:
// `GEODA_ERR_INVALID_ARGUMENT` si la columna no tiene valores validos y
// `GEODA_ERR_UNSUPPORTED` para `Bool` y `Utf8`.
//
// # Safety
// `column` debe ser un handle valido y `out` debe ser escribible. Ninguno puede ser
// NULL (si lo es, se devuelve un error).
int32_t geoda_column_median(const struct GeodaColumn *column, double *out);

// Percentil `p` (en `[0, 100]`, interpolacion lineal tipo 7) de una columna `Int32`,
// `Int64` o `Float64`, en `*out`.
//
// Mismas reglas que `geoda_column_median`; ademas, un `p` fuera de `[0, 100]` (o `NaN`)
// es `GEODA_ERR_INVALID_ARGUMENT`.
//
// # Safety
// `column` debe ser un handle valido y `out` debe ser escribible. Ninguno puede ser
// NULL (si lo es, se devuelve un error).
int32_t geoda_column_percentile(const struct GeodaColumn *column, double p, double *out);

// Cantidad de valores no nulos de una columna `Int32`, `Int64` o `Float64`, en `*out`.
//
// Si falla, devuelve un codigo negativo y `*out` no se escribe: `GEODA_ERR_UNSUPPORTED`
// para `Bool` y `Utf8`.
//
// # Safety
// `column` debe ser un handle valido y `out` debe ser escribible. Ninguno puede ser NULL
// (si lo es, se devuelve un error).
int32_t geoda_column_count(const struct GeodaColumn *column, int64_t *out);

// Suma de una columna `Int32` o `Int64`, en `*out` (`i64`). Sin valores validos es 0.
//
// Los nulos se ignoran. Si falla, devuelve un codigo negativo y `*out` no se escribe:
// `GEODA_ERR_OUT_OF_BOUNDS` si la suma no cabe en `i64`, `GEODA_ERR_TYPE_MISMATCH` para
// `Float64` (usar `geoda_column_sum_f64`) y `GEODA_ERR_UNSUPPORTED` para `Bool` y `Utf8`.
//
// `workers` es el numero de hilos (al menos 1): el resultado no depende de el. `0` es
// `GEODA_ERR_INVALID_ARGUMENT`.
//
// # Safety
// `column` debe ser un handle valido y `out` debe ser escribible. Ninguno puede ser NULL
// (si lo es, se devuelve un error).
int32_t geoda_column_sum_i64(const struct GeodaColumn *column, size_t workers, int64_t *out);

// Suma de una columna `Float64`, en `*out`. Sin valores validos es 0; un `NaN` da `NaN`.
//
// Los nulos se ignoran. `GEODA_ERR_TYPE_MISMATCH` para `Int32` e `Int64` (usar
// `geoda_column_sum_i64`); el resto de errores, como en `geoda_column_sum_i64`.
//
// `workers` es el numero de hilos (al menos 1): el resultado no depende de el. `0` es
// `GEODA_ERR_INVALID_ARGUMENT`.
//
// # Safety
// Igual que `geoda_column_sum_i64`.
int32_t geoda_column_sum_f64(const struct GeodaColumn *column, size_t workers, double *out);

// Minimo de una columna `Int32` o `Int64`, en `*out` (`i64`).
//
// Los nulos se ignoran. Si falla, devuelve un codigo negativo y `*out` no se escribe:
// `GEODA_ERR_INVALID_ARGUMENT` sin valores validos, `GEODA_ERR_TYPE_MISMATCH` para
// `Float64` (usar `geoda_column_min_f64`) y `GEODA_ERR_UNSUPPORTED` para `Bool` y `Utf8`.
//
// `workers` es el numero de hilos (al menos 1): el resultado no depende de el. `0` es
// `GEODA_ERR_INVALID_ARGUMENT`.
//
// # Safety
// `column` debe ser un handle valido y `out` debe ser escribible. Ninguno puede ser NULL
// (si lo es, se devuelve un error).
int32_t geoda_column_min_i64(const struct GeodaColumn *column, size_t workers, int64_t *out);

// Minimo de una columna `Float64`, en `*out`. Un `NaN` en la columna da `NaN`.
//
// Mismas reglas que `geoda_column_min_i64`, con `GEODA_ERR_TYPE_MISMATCH` para `Int32` e
// `Int64`.
//
// `workers` es el numero de hilos (al menos 1): el resultado no depende de el. `0` es
// `GEODA_ERR_INVALID_ARGUMENT`.
//
// # Safety
// Igual que `geoda_column_min_i64`.
int32_t geoda_column_min_f64(const struct GeodaColumn *column, size_t workers, double *out);

// Maximo de una columna `Int32` o `Int64`, en `*out` (`i64`). Mismas reglas que
// `geoda_column_min_i64`.
//
// `workers` es el numero de hilos (al menos 1): el resultado no depende de el. `0` es
// `GEODA_ERR_INVALID_ARGUMENT`.
//
// # Safety
// Igual que `geoda_column_min_i64`.
int32_t geoda_column_max_i64(const struct GeodaColumn *column, size_t workers, int64_t *out);

// Maximo de una columna `Float64`, en `*out`. Mismas reglas que `geoda_column_min_f64`.
//
// `workers` es el numero de hilos (al menos 1): el resultado no depende de el. `0` es
// `GEODA_ERR_INVALID_ARGUMENT`.
//
// # Safety
// Igual que `geoda_column_min_i64`.
int32_t geoda_column_max_f64(const struct GeodaColumn *column, size_t workers, double *out);

// Media de una columna `Int32`, `Int64` o `Float64`, en `*out` (siempre `f64`).
//
// Los nulos se ignoran; un `NaN` da `NaN`. Si falla, devuelve un codigo negativo y `*out`
// no se escribe: `GEODA_ERR_INVALID_ARGUMENT` sin valores validos y
// `GEODA_ERR_UNSUPPORTED` para `Bool` y `Utf8`.
//
// `workers` es el numero de hilos (al menos 1): el resultado no depende de el. `0` es
// `GEODA_ERR_INVALID_ARGUMENT`.
//
// # Safety
// `column` debe ser un handle valido y `out` debe ser escribible. Ninguno puede ser NULL
// (si lo es, se devuelve un error).
int32_t geoda_column_mean(const struct GeodaColumn *column, size_t workers, double *out);

// Varianza de una columna numerica con `ddof` grados de libertad restados (`0` =
// poblacional, `1` = muestral), en `*out`.
//
// `ddof` es obligatorio: la capi no tiene valor por defecto (S2). Mismas reglas que
// `geoda_column_mean`; ademas, `ddof >= n` (con `n` los valores validos) es
// `GEODA_ERR_INVALID_ARGUMENT`.
//
// `workers` es el numero de hilos (al menos 1): el resultado no depende de el. `0` es
// `GEODA_ERR_INVALID_ARGUMENT`.
//
// # Safety
// `column` debe ser un handle valido y `out` debe ser escribible. Ninguno puede ser NULL
// (si lo es, se devuelve un error).
int32_t geoda_column_var(const struct GeodaColumn *column,
                         size_t ddof,
                         size_t workers,
                         double *out);

// Desviacion estandar (`sqrt` de la varianza) con `ddof` grados de libertad restados, en
// `*out`. Mismas reglas que `geoda_column_var`.
//
// # Safety
// Igual que `geoda_column_var`.
int32_t geoda_column_std(const struct GeodaColumn *column,
                         size_t ddof,
                         size_t workers,
                         double *out);

// Correlacion de Pearson entre dos columnas numericas, en `*out`, acotada a `[-1, 1]`.
//
// Las filas con nulo en cualquiera de las dos se descartan en ambas (S7). Si falla,
// devuelve un codigo negativo y `*out` no se escribe: `GEODA_ERR_LENGTH_MISMATCH` si miden
// distinto, `GEODA_ERR_INVALID_ARGUMENT` si una columna es constante o no hay filas
// validas, y `GEODA_ERR_UNSUPPORTED` para `Bool` y `Utf8`. Un `NaN` da `NaN`.
//
// `workers` es el numero de hilos (al menos 1): el resultado no depende de el. `0` es
// `GEODA_ERR_INVALID_ARGUMENT`.
//
// # Safety
// `x`, `y` deben ser handles validos y `out` debe ser escribible. Ninguno puede ser NULL
// (si lo es, se devuelve un error).
int32_t geoda_column_corr(const struct GeodaColumn *x,
                          const struct GeodaColumn *y,
                          size_t workers,
                          double *out);

// Histograma de ancho fijo de una columna `Int32`, `Int64` o `Float64`, con `n_bins` cubos
// iguales. Devuelve **tres columnas nuevas** (S9):
//
// - `*out_edges`: `Float64`, `n_bins + 1` bordes. El cubo `i` es `[edges[i], edges[i+1])` y
//   el ultimo es cerrado.
// - `*out_counts`: `Int64`, `n_bins` conteos.
// - `*out_outside`: `Int64`, 4 valores `[below, above, nulls, nans]` (lo que no cae en ningun
//   cubo se cuenta aparte, no se pierde).
//
// Con `has_range` en `true` el rango es `[lo, hi]`; en `false`, `lo` y `hi` se ignoran y el
// rango es `[min, max]` de los datos (sin contar nulos ni `NaN`). Con `lo == hi` el rango se
// ensancha a `[lo - 0.5, hi + 0.5]`. Van por valor y no por puntero: asi la funcion no lee
// memoria del llamador fuera de los handles.
//
// Cada columna debe liberarse con `geoda_column_free`. Si falla, devuelve un codigo negativo
// y los tres `out` quedan en NULL: `GEODA_ERR_INVALID_ARGUMENT` si `n_bins` es 0 o enorme, el
// rango esta invertido o no es finito, o no hay valores validos y `has_range` es `false`; y
// `GEODA_ERR_UNSUPPORTED` para `Bool` y `Utf8`.
//
// `workers` es el numero de hilos (al menos 1): el resultado no depende de el. `0` es
// `GEODA_ERR_INVALID_ARGUMENT`.
//
// # Safety
// `column` debe ser un handle valido y los tres `out` deben ser escribibles. Ninguno puede ser
// NULL (si lo es, se devuelve un error).
int32_t geoda_column_histogram(const struct GeodaColumn *column,
                               size_t n_bins,
                               bool has_range,
                               double lo,
                               double hi,
                               size_t workers,
                               struct GeodaColumn **out_edges,
                               struct GeodaColumn **out_counts,
                               struct GeodaColumn **out_outside);

// Agrupa por los valores de `keys` y calcula un agregado de `values` en cada grupo. Devuelve
// **dos columnas nuevas**:
//
// - `*out_keys`: del mismo tipo que `keys`, una fila por grupo en orden de primera aparicion.
//   Las claves nulas forman su propio grupo (sale como nulo, S8); `NaN` es una sola clave y
//   `-0.0` y `0.0` son distintas.
// - `*out_values`: `Int64` para `GEODA_AGG_COUNT` y para suma/minimo/maximo sobre enteros;
//   `Float64` en los demas casos.
//
// `agg` es una de las constantes `GEODA_AGG_*`; `ddof` solo cuenta en `GEODA_AGG_VAR` y
// `GEODA_AGG_STD`. Los nulos de `values` se ignoran en cada grupo.
//
// Cada columna debe liberarse con `geoda_column_free`. Si falla, devuelve un codigo negativo
// y los dos `out` quedan en NULL: `GEODA_ERR_LENGTH_MISMATCH` si `keys` y `values` miden
// distinto, `GEODA_ERR_INVALID_ARGUMENT` si `agg` no existe o un grupo no tiene valores
// validos para `mean`/`min`/`max`/`var`/`std` (o `ddof >= n`), y `GEODA_ERR_UNSUPPORTED` si
// `values` es `Bool` o `Utf8`.
//
// `workers` es el numero de hilos (al menos 1): el resultado no depende de el. `0` es
// `GEODA_ERR_INVALID_ARGUMENT`.
//
// # Safety
// `keys` y `values` deben ser handles validos y los dos `out` deben ser escribibles. Ninguno
// puede ser NULL (si lo es, se devuelve un error).
int32_t geoda_column_group_by(const struct GeodaColumn *keys,
                              const struct GeodaColumn *values,
                              int32_t agg,
                              size_t ddof,
                              size_t workers,
                              struct GeodaColumn **out_keys,
                              struct GeodaColumn **out_values);

// Construye un grafo de `n` nodos desde dos columnas `Int64` de aristas (`from[i] -> to[i]`) y,
// opcionalmente, una columna `Float64` de pesos (`weights` NULL = grafo sin pesos). El grafo se
// libera con `geoda_graph_free`.
//
// Si falla devuelve un codigo negativo y `*out` queda en NULL: `GEODA_ERR_TYPE_MISMATCH` si una
// columna no tiene el tipo pedido, `GEODA_ERR_LENGTH_MISMATCH` si miden distinto,
// `GEODA_ERR_INVALID_ARGUMENT` si hay un nulo, y `GEODA_ERR_OUT_OF_BOUNDS` si un nodo es negativo
// o `>= n`, o el grafo no cabe en `u32`.
//
// # Safety
// `from`, `to` y `weights` (si no es NULL) deben ser handles validos y `out` debe ser escribible.
// `from`, `to` y `out` no pueden ser NULL (si lo son, se devuelve un error).
int32_t geoda_graph_new(size_t n,
                        const struct GeodaColumn *from,
                        const struct GeodaColumn *to,
                        const struct GeodaColumn *weights,
                        bool directed,
                        struct GeodaGraph **out);

// Libera un grafo. Acepta NULL (no hace nada). Despues de llamarla el handle ya no es valido;
// liberarlo dos veces es comportamiento indefinido.
//
// # Safety
// `graph` debe ser NULL o un handle devuelto por `geoda_graph_new` y no liberado aun.
void geoda_graph_free(struct GeodaGraph *graph);

// Cantidad de nodos del grafo, o un codigo de error negativo si `graph` es NULL.
//
// # Safety
// `graph` debe ser NULL o un handle valido.
int64_t geoda_graph_node_count(const struct GeodaGraph *graph);

// Recorrido en anchura desde `source`. Devuelve **tres columnas `Int64` nuevas**:
//
// - `*out_order`: los nodos alcanzables en el orden en que se visitaron.
// - `*out_distance`: una fila por nodo, con la distancia en aristas desde `source`; nulo si el
//   nodo no es alcanzable.
// - `*out_parent`: una fila por nodo, con el nodo desde el que se descubrio; nulo en `source`
//   y en los no alcanzables.
//
// Cada columna debe liberarse con `geoda_column_free`. Si falla, devuelve un codigo negativo y
// los tres `out` quedan en NULL: `GEODA_ERR_OUT_OF_BOUNDS` si `source >= n`.
//
// # Safety
// `graph` debe ser un handle valido y los tres `out` deben ser escribibles. Ninguno puede ser
// NULL (si lo es, se devuelve un error).
int32_t geoda_graph_bfs(const struct GeodaGraph *graph,
                        size_t source,
                        struct GeodaColumn **out_order,
                        struct GeodaColumn **out_distance,
                        struct GeodaColumn **out_parent);

// Componentes conexas (debilmente conexas en un grafo dirigido). Devuelve una columna `Int64`
// nueva por `*out_ids` con el id de componente de cada nodo (desde 0, en el orden del primer
// nodo de cada una) y escribe en `*out_count` cuantas hay.
//
// `*out_ids` se libera con `geoda_column_free`. Si falla, devuelve un codigo negativo, `*out_ids`
// queda en NULL y `*out_count` no se escribe.
//
// # Safety
// `graph` debe ser un handle valido; `out_ids` y `out_count` deben ser escribibles y no NULL.
int32_t geoda_graph_components(const struct GeodaGraph *graph,
                               struct GeodaColumn **out_ids,
                               int64_t *out_count);

// Orden topologico de un grafo dirigido sin ciclos: una columna `Int64` nueva por `*out_order`
// con los nodos en orden (entre varios posibles gana el de menor indice).
//
// Se libera con `geoda_column_free`. Si falla, `*out_order` queda en NULL:
// `GEODA_ERR_INVALID_ARGUMENT` si hay un ciclo (un bucle cuenta) o el grafo no es dirigido.
//
// # Safety
// `graph` debe ser un handle valido y `out_order` escribible y no NULL.
int32_t geoda_graph_topological_order(const struct GeodaGraph *graph,
                                      struct GeodaColumn **out_order);

// Caminos minimos desde `source` en un grafo con pesos no negativos. Devuelve **dos columnas
// nuevas**:
//
// - `*out_distance`: `Float64`, una fila por nodo; `+inf` si no es alcanzable.
// - `*out_parent`: `Int64`, una fila por nodo; el nodo anterior en un camino minimo, nulo en
//   `source` y en los no alcanzables.
//
// Cada columna debe liberarse con `geoda_column_free`. Si falla, devuelve un codigo negativo y
// los dos `out` quedan en NULL: `GEODA_ERR_TYPE_MISMATCH` si el grafo no tiene pesos,
// `GEODA_ERR_INVALID_ARGUMENT` si algun peso es negativo o `NaN`, y `GEODA_ERR_OUT_OF_BOUNDS` si
// `source >= n`.
//
// # Safety
// `graph` debe ser un handle valido y los dos `out` deben ser escribibles y no NULL.
int32_t geoda_graph_dijkstra(const struct GeodaGraph *graph,
                             size_t source,
                             struct GeodaColumn **out_distance,
                             struct GeodaColumn **out_parent);

// Bosque de expansion minimo (Kruskal) de un grafo **no dirigido** con pesos. Devuelve una
// columna `Int64` nueva por `*out_edges` con las posiciones, en la lista con la que se creo el
// grafo, de las aristas elegidas (de menor a mayor peso; con el mismo peso gana la primera de
// la lista); escribe en `*out_total` la suma de sus pesos y en `*out_components` cuantas
// componentes tiene el grafo.
//
// `*out_edges` se libera con `geoda_column_free`. Si falla, devuelve un codigo negativo,
// `*out_edges` queda en NULL y los otros dos `out` no se escriben: `GEODA_ERR_TYPE_MISMATCH` si
// el grafo no tiene pesos, `GEODA_ERR_INVALID_ARGUMENT` si es dirigido o algun peso es `NaN`.
//
// # Safety
// `graph` debe ser un handle valido y los tres `out` deben ser escribibles y no NULL.
int32_t geoda_graph_mst(const struct GeodaGraph *graph,
                        struct GeodaColumn **out_edges,
                        double *out_total,
                        int64_t *out_components);

// Distancia de edicion (Levenshtein, por caracteres Unicode) fila a fila entre dos columnas
// `Utf8`. Las dos miden lo mismo, o una mide 1 y se compara con todas las filas de la otra.
// Una fila nula en cualquiera da nulo. Devuelve una columna `Int64` nueva por `*out`.
//
// `*out` se libera con `geoda_column_free`. Si falla, devuelve un codigo negativo y `*out`
// queda en NULL: `GEODA_ERR_TYPE_MISMATCH` si alguna no es `Utf8` y
// `GEODA_ERR_LENGTH_MISMATCH` si miden distinto y ninguna mide 1.
//
// # Safety
// `a` y `b` deben ser handles validos y `out` escribible y no NULL.
int32_t geoda_column_edit_distance(const struct GeodaColumn *a,
                                   const struct GeodaColumn *b,
                                   struct GeodaColumn **out);

// Subsecuencia estrictamente creciente mas larga (LIS) de una columna `Int32`, `Int64` o
// `Float64`. Devuelve una columna `Int64` nueva por `*out` con las posiciones de sus elementos,
// de menor a mayor; su largo es el de la LIS.
//
// `*out` se libera con `geoda_column_free`. Si falla, devuelve un codigo negativo y `*out` queda
// en NULL: `GEODA_ERR_INVALID_ARGUMENT` si hay algun nulo o `NaN`, y `GEODA_ERR_UNSUPPORTED`
// para `Bool` y `Utf8`.
//
// # Safety
// `column` debe ser un handle valido y `out` escribible y no NULL.
int32_t geoda_column_lis(const struct GeodaColumn *column, struct GeodaColumn **out);

// Codifica con Huffman una columna `Int32` de bytes (valores `0..=255`, sin nulos). Devuelve
// **dos columnas nuevas**:
//
// - `*out_bits`: `Bool`, los bits codificados (una fila por bit, el primero es el de la raiz).
// - `*out_frequencies`: `Int64`, 256 filas con cuantas veces aparece cada byte. Es todo lo que
//   `geoda_huffman_decode` necesita: la tabla se reconstruye de ahi, con desempate fijo.
//
// Cada columna debe liberarse con `geoda_column_free`. Si falla, devuelve un codigo negativo y
// los dos `out` quedan en NULL: `GEODA_ERR_TYPE_MISMATCH` si no es `Int32` y
// `GEODA_ERR_INVALID_ARGUMENT` si hay nulos o un valor fuera de `0..=255`.
//
// # Safety
// `data` debe ser un handle valido y los dos `out` deben ser escribibles y no NULL.
int32_t geoda_huffman_encode(const struct GeodaColumn *data,
                             struct GeodaColumn **out_bits,
                             struct GeodaColumn **out_frequencies);

// Decodifica con Huffman una columna `Bool` de bits, con la tabla que sale de `frequencies`
// (`Int64`, 256 filas no negativas, las de `geoda_huffman_encode`). Devuelve una columna
// `Int32` nueva por `*out_data` con los bytes.
//
// `*out_data` se libera con `geoda_column_free`. Si falla, devuelve un codigo negativo y
// `*out_data` queda en NULL: `GEODA_ERR_TYPE_MISMATCH` si los tipos no son `Int64` y `Bool`, y
// `GEODA_ERR_INVALID_ARGUMENT` si hay nulos, las frecuencias no son 256 valores no negativos o
// los bits no se pueden decodificar con esa tabla (terminan a media palabra, o no hay simbolos).
//
// # Safety
// `frequencies` y `bits` deben ser handles validos y `out_data` escribible y no NULL.
int32_t geoda_huffman_decode(const struct GeodaColumn *frequencies,
                             const struct GeodaColumn *bits,
                             struct GeodaColumn **out_data);

// Agrupa las filas de una columna `Utf8` que son casi iguales: dos filas estan cerca si su
// distancia de edicion (por caracteres Unicode) es a lo sumo `max_distance`, y un grupo es una
// componente conexa de «estar cerca» (si `a~b` y `b~c`, los tres van juntos). Devuelve una
// columna `Int64` nueva por `*out_ids` con el grupo de cada fila (desde 0, por primera
// aparicion; nulo en las filas nulas) y escribe en `*out_count` cuantos grupos hay.
//
// `*out_ids` se libera con `geoda_column_free`. Si falla, devuelve un codigo negativo,
// `*out_ids` queda en NULL y `*out_count` no se escribe: `GEODA_ERR_TYPE_MISMATCH` si no es
// `Utf8`.
//
// # Safety
// `column` debe ser un handle valido; `out_ids` y `out_count` deben ser escribibles y no NULL.
int32_t geoda_column_dedupe(const struct GeodaColumn *column,
                            size_t max_distance,
                            struct GeodaColumn **out_ids,
                            int64_t *out_count);

// K-means sobre una matriz de `n_columns` columnas (`Int32`, `Int64` o `Float64`, sin nulos ni
// `NaN`), con inicializacion k-means++ y `seed` explicita: la misma entrada, `k` y semilla dan el
// mismo resultado. Devuelve **dos columnas nuevas** y dos escalares:
//
// - `*out_labels`: `Int64`, el grupo (`0..k`) de cada fila.
// - `*out_centroids`: `Float64`, `k * n_columns` valores **por filas** (el centroide `c` ocupa
//   las posiciones `c * n_columns .. (c + 1) * n_columns`).
// - `*out_inertia`: suma de distancias al cuadrado de cada fila a su centroide.
// - `*out_iterations`: iteraciones hechas.
//
// `max_iter` es el tope de iteraciones (al menos 1) y `tol` el movimiento maximo de un centroide
// con el que se da por convergido (finito y no negativo).
//
// Si falla, devuelve un codigo negativo, las dos columnas quedan en NULL y los escalares no se
// escriben: `GEODA_ERR_INVALID_ARGUMENT` si `columns` es NULL o `n_columns` es 0, hay nulos o
// valores no finitos, `k` es 0 o mayor que las filas, o `max_iter`/`tol` no valen;
// `GEODA_ERR_LENGTH_MISMATCH` si las columnas miden distinto.
//
// # Safety
// `columns` debe apuntar a `n_columns` handles validos; los cuatro `out` deben ser escribibles y
// no NULL. Las columnas de salida se liberan con `geoda_column_free`.
int32_t geoda_kmeans(const struct GeodaColumn *const *columns,
                     size_t n_columns,
                     size_t k,
                     size_t max_iter,
                     double tol,
                     uint64_t seed,
                     struct GeodaColumn **out_labels,
                     struct GeodaColumn **out_centroids,
                     double *out_inertia,
                     int64_t *out_iterations);

// Dendrograma de enlace simple (clustering jerarquico con distancia euclidiana) de una matriz de
// `n_columns` columnas (`Int32`, `Int64` o `Float64`, sin nulos ni `NaN`) de hasta 5000 filas.
// Devuelve **cuatro columnas nuevas** con `n - 1` filas cada una (ninguna si hay 1 fila):
//
// - `*out_left` y `*out_right`: `Int64`, los dos grupos que se unen (`left < right`). Los grupos
//   `0..n` son las filas; la union numero `s` crea el grupo `n + s`.
// - `*out_height`: `Float64`, la distancia a la que se unen (no decrece).
// - `*out_size`: `Int64`, las filas del grupo resultante.
//
// Si falla, devuelve un codigo negativo y los cuatro `out` quedan en NULL:
// `GEODA_ERR_INVALID_ARGUMENT` si `columns` es NULL o `n_columns` es 0, hay nulos o no finitos, o
// hay mas de 5000 filas; `GEODA_ERR_LENGTH_MISMATCH` si las columnas miden distinto.
//
// # Safety
// `columns` debe apuntar a `n_columns` handles validos; los cuatro `out` deben ser escribibles y
// no NULL. Cada columna se libera con `geoda_column_free`.
int32_t geoda_hierarchical(const struct GeodaColumn *const *columns,
                           size_t n_columns,
                           struct GeodaColumn **out_left,
                           struct GeodaColumn **out_right,
                           struct GeodaColumn **out_height,
                           struct GeodaColumn **out_size);

// Corta en `k` grupos un dendrograma de `n` filas dado por sus uniones (`left` y `right`, las dos
// `Int64` de `n - 1` filas, como las devuelve `geoda_hierarchical`). Devuelve una columna `Int64`
// nueva por `*out_labels` con el grupo (`0..k`) de cada fila, numerados por la primera fila en
// que aparece cada uno.
//
// `*out_labels` se libera con `geoda_column_free`. Si falla, devuelve un codigo negativo y
// `*out_labels` queda en NULL: `GEODA_ERR_INVALID_ARGUMENT` si `k` es 0 o mayor que `n`, las
// columnas no tienen `n - 1` filas, hay nulos o una union usa un grupo que aun no existe;
// `GEODA_ERR_OUT_OF_BOUNDS` si hay un grupo negativo y `GEODA_ERR_TYPE_MISMATCH` si no son `Int64`.
//
// # Safety
// `left` y `right` deben ser handles validos y `out_labels` escribible y no NULL.
int32_t geoda_dendrogram_cut(const struct GeodaColumn *left,
                             const struct GeodaColumn *right,
                             size_t n,
                             size_t k,
                             struct GeodaColumn **out_labels);

// Entrena un arbol de decision de clasificacion (CART con impureza de Gini) con una matriz de
// `n_columns` columnas (`Int32`, `Int64` o `Float64`, sin nulos ni `NaN`) y una columna `Int64` de
// etiquetas (`>= 0`, sin nulos; pueden ser dispersas). Devuelve un handle nuevo por `*out`.
//
// `max_depth` es la profundidad maxima (0 = una sola hoja) y `min_samples_leaf` las filas
// minimas por hoja (al menos 1). El arbol es determinista: la misma entrada da el mismo arbol.
//
// Se libera con `geoda_tree_free`. Si falla, devuelve un codigo negativo y `*out` queda en NULL:
// `GEODA_ERR_INVALID_ARGUMENT` si `columns` es NULL o `n_columns` es 0, no hay filas, hay nulos o
// no finitos, una etiqueta es negativa o `min_samples_leaf` es 0; `GEODA_ERR_LENGTH_MISMATCH` si
// las columnas o las etiquetas miden distinto; `GEODA_ERR_TYPE_MISMATCH` si las etiquetas no son
// `Int64`.
//
// # Safety
// `columns` debe apuntar a `n_columns` handles validos, `labels` ser un handle valido y `out`
// escribible y no NULL.
int32_t geoda_tree_fit(const struct GeodaColumn *const *columns,
                       size_t n_columns,
                       const struct GeodaColumn *labels,
                       size_t max_depth,
                       size_t min_samples_leaf,
                       struct GeodaTree **out);

// Predice la clase de cada fila de una matriz de `n_columns` columnas. Devuelve una columna
// `Int64` nueva por `*out_labels` con las mismas etiquetas con las que se entreno.
//
// `*out_labels` se libera con `geoda_column_free`. Si falla, devuelve un codigo negativo y
// `*out_labels` queda en NULL: `GEODA_ERR_INVALID_ARGUMENT` si `tree` o `columns` son NULL,
// `n_columns` es 0, hay nulos o no finitos; `GEODA_ERR_LENGTH_MISMATCH` si `n_columns` no es el
// numero de variables del entrenamiento o las columnas miden distinto.
//
// # Safety
// `tree` debe ser un handle valido, `columns` apuntar a `n_columns` handles validos y
// `out_labels` ser escribible y no NULL.
int32_t geoda_tree_predict(const struct GeodaTree *tree,
                           const struct GeodaColumn *const *columns,
                           size_t n_columns,
                           struct GeodaColumn **out_labels);

// Cantidad de nodos del arbol (hojas incluidas), o un codigo de error negativo si `tree` es NULL.
//
// # Safety
// `tree` debe ser NULL o un handle valido.
int64_t geoda_tree_node_count(const struct GeodaTree *tree);

// Libera un arbol. Acepta NULL (no hace nada). Despues de llamarla el handle ya no es valido;
// liberarlo dos veces es comportamiento indefinido.
//
// # Safety
// `tree` debe ser NULL o un handle devuelto por `geoda_tree_fit` y no liberado aun.
void geoda_tree_free(struct GeodaTree *tree);

// Conjuntos de items frecuentes (algoritmo Apriori) de unas transacciones en **formato largo**:
// `transaction[i]` contiene a `item[i]` (dos columnas `Int64` del mismo largo, ids `>= 0`, sin
// nulos; un item repetido dentro de una transaccion cuenta una vez). `min_support` es un conteo
// absoluto de transacciones (al menos 1). Devuelve **tres columnas `Int64` nuevas**:
//
// - `*out_set` y `*out_item`: los conjuntos en formato largo, una fila por item. `set` es el id
//   del conjunto (desde 0); los ids siguen el orden por tamano y luego lexicografico, y los items
//   de cada conjunto van de menor a mayor.
// - `*out_support`: una fila por conjunto, con las transacciones que lo contienen.
//
// Cada columna debe liberarse con `geoda_column_free`. Si falla, devuelve un codigo negativo y los
// tres `out` quedan en NULL: `GEODA_ERR_INVALID_ARGUMENT` si `min_support` es 0, hay nulos o ids
// negativos; `GEODA_ERR_LENGTH_MISMATCH` si las columnas miden distinto; `GEODA_ERR_TYPE_MISMATCH`
// si no son `Int64`.
//
// # Safety
// `transaction` e `item` deben ser handles validos y los tres `out` escribibles y no NULL.
int32_t geoda_apriori_itemsets(const struct GeodaColumn *transaction,
                               const struct GeodaColumn *item,
                               size_t min_support,
                               struct GeodaColumn **out_set,
                               struct GeodaColumn **out_item,
                               struct GeodaColumn **out_support);

// Reglas de asociacion `antecedente -> consecuente` de unas transacciones en formato largo (ver
// `geoda_apriori_itemsets`), con soporte `>= min_support` y confianza `>= min_confidence`
// (en `(0, 1]`). Devuelve **cinco columnas nuevas**, una fila por regla:
//
// - `*out_antecedent` y `*out_consequent`: `Int64`, el id del conjunto del antecedente y del
//   consecuente. Son ids de los conjuntos que devuelve `geoda_apriori_itemsets` con el mismo
//   `min_support` (los dos son siempre frecuentes).
// - `*out_support`: `Int64`, transacciones que contienen antecedente y consecuente.
// - `*out_confidence`: `Float64`, `soporte(A u C) / soporte(A)`.
// - `*out_lift`: `Float64`, `confianza / (soporte(C) / N)` con `N` las transacciones.
//
// Las reglas salen por conjunto frecuente (en el orden de `geoda_apriori_itemsets`) y, dentro de
// cada uno, por la mascara de bits del antecedente. Cada columna debe liberarse con
// `geoda_column_free`. Si falla, devuelve un codigo negativo y los cinco `out` quedan en NULL: los
// mismos errores que `geoda_apriori_itemsets`, y `GEODA_ERR_INVALID_ARGUMENT` si `min_confidence`
// no esta en `(0, 1]` o un conjunto frecuente tiene mas de 20 items.
//
// # Safety
// `transaction` e `item` deben ser handles validos y los cinco `out` escribibles y no NULL.
int32_t geoda_apriori_rules(const struct GeodaColumn *transaction,
                            const struct GeodaColumn *item,
                            size_t min_support,
                            double min_confidence,
                            struct GeodaColumn **out_antecedent,
                            struct GeodaColumn **out_consequent,
                            struct GeodaColumn **out_support,
                            struct GeodaColumn **out_confidence,
                            struct GeodaColumn **out_lift);

// Producto de matrices `A * B`. Cada matriz de entrada es una lista de columnas (`n_a` y `n_b`
// handles de `Int32`, `Int64` o `Float64`, sin nulos ni valores no finitos): el handle `j` es la
// columna `j` de la matriz. Devuelve una columna `Float64` nueva por `*out` con las
// `filas(A) * n_b` entradas del producto **por filas**.
//
// `*out` se libera con `geoda_column_free`. Si falla, devuelve un codigo negativo y `*out` queda en
// NULL: `GEODA_ERR_INVALID_ARGUMENT` si una lista es NULL o vacia, hay nulos o no finitos;
// `GEODA_ERR_LENGTH_MISMATCH` si las columnas de una matriz miden distinto o `n_a` no es igual a las
// filas de `B`.
//
// # Safety
// `a` debe apuntar a `n_a` handles validos, `b` a `n_b` handles validos y `out` ser escribible y no
// NULL.
int32_t geoda_matmul(const struct GeodaColumn *const *a,
                     size_t n_a,
                     const struct GeodaColumn *const *b,
                     size_t n_b,
                     struct GeodaColumn **out);

// Resuelve `A x = b` por LU con pivoteo parcial. `a` es una matriz cuadrada de `n_a` columnas y `b`
// una columna (`Int32`, `Int64` o `Float64`, sin nulos) con tantas filas como `A`. Devuelve una
// columna `Float64` nueva por `*out` con `x`.
//
// `*out` se libera con `geoda_column_free`. Si falla, devuelve un codigo negativo y `*out` queda en
// NULL: `GEODA_ERR_INVALID_ARGUMENT` si `A` es singular o hay nulos o no finitos;
// `GEODA_ERR_LENGTH_MISMATCH` si `A` no es cuadrada o `b` mide distinto.
//
// # Safety
// `a` debe apuntar a `n_a` handles validos, `b` ser un handle valido y `out` escribible y no NULL.
int32_t geoda_lu_solve(const struct GeodaColumn *const *a,
                       size_t n_a,
                       const struct GeodaColumn *b,
                       struct GeodaColumn **out);

// Determinante de una matriz cuadrada de `n_a` columnas, por LU con pivoteo parcial. Una matriz
// singular tiene determinante `0` (no es un error). Escribe el resultado en `*out`.
//
// Si falla, devuelve un codigo negativo y `*out` no se escribe: `GEODA_ERR_INVALID_ARGUMENT` si la
// lista es NULL o vacia, hay nulos o no finitos; `GEODA_ERR_LENGTH_MISMATCH` si no es cuadrada.
//
// # Safety
// `a` debe apuntar a `n_a` handles validos y `out` ser escribible y no NULL.
int32_t geoda_det(const struct GeodaColumn *const *a, size_t n_a, double *out);

// Inversa de una matriz cuadrada de `n_a` columnas. Devuelve una columna `Float64` nueva por
// `*out` con las `n_a * n_a` entradas **por filas**.
//
// `*out` se libera con `geoda_column_free`. Si falla, devuelve un codigo negativo y `*out` queda en
// NULL: `GEODA_ERR_INVALID_ARGUMENT` si es singular o hay nulos o no finitos;
// `GEODA_ERR_LENGTH_MISMATCH` si no es cuadrada.
//
// # Safety
// `a` debe apuntar a `n_a` handles validos y `out` ser escribible y no NULL.
int32_t geoda_inverse(const struct GeodaColumn *const *a, size_t n_a, struct GeodaColumn **out);

// Factorizacion QR delgada `A = Q R` por reflexiones de Householder. `a` es una matriz de `m x n`
// (`n_a = n` columnas de `m` filas, con `m >= n`). Devuelve **dos columnas `Float64` nuevas**, planas
// y por filas:
//
// - `*out_q`: `m * n` valores (columnas ortonormales).
// - `*out_r`: `n * n` valores (triangular superior).
//
// Si falla, devuelve un codigo negativo y los dos `out` quedan en NULL: `GEODA_ERR_INVALID_ARGUMENT`
// si la lista es NULL o vacia, hay nulos o no finitos, o `m < n`; `GEODA_ERR_LENGTH_MISMATCH` si las
// columnas miden distinto.
//
// # Safety
// `a` debe apuntar a `n_a` handles validos y los dos `out` deben ser escribibles y no NULL.
int32_t geoda_qr(const struct GeodaColumn *const *a,
                 size_t n_a,
                 struct GeodaColumn **out_q,
                 struct GeodaColumn **out_r);

// SVD delgada `A = U S V^T` (Jacobi unilateral) de una matriz de `m x n` (`n_a = n` columnas de `m`
// filas). Con `k = min(m, n)` devuelve **tres columnas `Float64` nuevas**:
//
// - `*out_u`: `m * k` valores por filas (columnas ortonormales).
// - `*out_s`: los `k` valores singulares, de mayor a menor.
// - `*out_vt`: `k * n` valores por filas (filas ortonormales).
//
// El signo es determinista: en cada columna de `V` la entrada de mayor valor absoluto es positiva.
// Si falla, devuelve un codigo negativo y los tres `out` quedan en NULL: `GEODA_ERR_INVALID_ARGUMENT`
// si la lista es NULL o vacia, hay nulos o no finitos, o la SVD no converge;
// `GEODA_ERR_LENGTH_MISMATCH` si las columnas miden distinto.
//
// # Safety
// `a` debe apuntar a `n_a` handles validos y los tres `out` deben ser escribibles y no NULL.
int32_t geoda_svd(const struct GeodaColumn *const *a,
                  size_t n_a,
                  struct GeodaColumn **out_u,
                  struct GeodaColumn **out_s,
                  struct GeodaColumn **out_vt);

// Minimos cuadrados `min |A x - b|` por QR de Householder. `a` es una matriz de `m x n` (`n_a = n`
// columnas, `m >= n`) de rango completo y `b` una columna de `m` valores. Devuelve una columna
// `Float64` nueva por `*out_x` con los `n` coeficientes y escribe en `*out_residual` la norma
// euclidiana de `A x - b`.
//
// Si falla, devuelve un codigo negativo, `*out_x` queda en NULL y `*out_residual` no se escribe:
// `GEODA_ERR_INVALID_ARGUMENT` si `m < n`, las columnas son dependientes (`rango incompleto`) o hay
// nulos o no finitos; `GEODA_ERR_LENGTH_MISMATCH` si `b` mide distinto que las filas de `A`.
//
// # Safety
// `a` debe apuntar a `n_a` handles validos, `b` ser un handle valido y `out_x` y `out_residual`
// escribibles y no NULL.
int32_t geoda_lstsq(const struct GeodaColumn *const *a,
                    size_t n_a,
                    const struct GeodaColumn *b,
                    struct GeodaColumn **out_x,
                    double *out_residual);

// Ajusta una regresion lineal `y = b0 + b1 x1 + ...` por minimos cuadrados (QR de Householder).
// `columns` son las `n_columns` variables explicativas (`Int32`, `Int64` o `Float64`, sin nulos ni
// `NaN`) e `y` la columna a explicar. Con `fit_intercept` el modelo lleva `b0`; sin el pasa por el
// origen. Devuelve un handle nuevo por `*out`.
//
// Se libera con `geoda_regression_free`. Si falla, devuelve un codigo negativo y `*out` queda en NULL:
// `GEODA_ERR_INVALID_ARGUMENT` si `columns` es NULL o `n_columns` es 0, hay nulos o no finitos, hay
// menos filas que coeficientes o las variables son colineales (`rango incompleto`);
// `GEODA_ERR_LENGTH_MISMATCH` si las columnas o `y` miden distinto.
//
// # Safety
// `columns` debe apuntar a `n_columns` handles validos, `y` ser un handle valido y `out` escribible y
// no NULL.
int32_t geoda_regression_fit(const struct GeodaColumn *const *columns,
                             size_t n_columns,
                             const struct GeodaColumn *y,
                             bool fit_intercept,
                             struct GeodaRegression **out);

// Predice `b0 + sum(bi * xi)` para cada fila de `n_columns` variables. Devuelve una columna `Float64`
// nueva por `*out`.
//
// `*out` se libera con `geoda_column_free`. Si falla, devuelve un codigo negativo y `*out` queda en
// NULL: `GEODA_ERR_INVALID_ARGUMENT` si `model` o `columns` son NULL, `n_columns` es 0 o hay nulos o no
// finitos; `GEODA_ERR_LENGTH_MISMATCH` si `n_columns` no es el numero de variables del ajuste o las
// columnas miden distinto.
//
// # Safety
// `model` debe ser un handle valido, `columns` apuntar a `n_columns` handles validos y `out`
// escribible y no NULL.
int32_t geoda_regression_predict(const struct GeodaRegression *model,
                                 const struct GeodaColumn *const *columns,
                                 size_t n_columns,
                                 struct GeodaColumn **out);

// Lee el modelo ajustado: devuelve una columna `Float64` nueva por `*out_coefficients` (un
// coeficiente por variable, en el orden de las columnas) y escribe el intercepto (`0` si se ajusto
// sin el) en `*out_intercept` y el `R^2` en `*out_r_squared`.
//
// `*out_coefficients` se libera con `geoda_column_free`. Si falla, devuelve un codigo negativo,
// `*out_coefficients` queda en NULL y los escalares no se escriben: `GEODA_ERR_INVALID_ARGUMENT` si
// `model` o un `out` es NULL.
//
// # Safety
// `model` debe ser un handle valido y los tres `out` escribibles y no NULL.
int32_t geoda_regression_params(const struct GeodaRegression *model,
                                struct GeodaColumn **out_coefficients,
                                double *out_intercept,
                                double *out_r_squared);

// Libera una regresion. Acepta NULL (no hace nada). Despues de llamarla el handle ya no es valido;
// liberarlo dos veces es comportamiento indefinido.
//
// # Safety
// `model` debe ser NULL o un handle devuelto por `geoda_regression_fit` y no liberado aun.
void geoda_regression_free(struct GeodaRegression *model);

// Ajusta un PCA de `n_components` componentes a `n_columns` variables (`Int32`, `Int64` o `Float64`,
// sin nulos ni `NaN`): centra cada variable (sin escalar) y toma la SVD de los datos centrados.
// Devuelve un handle nuevo por `*out`.
//
// Se libera con `geoda_pca_free`. Si falla, devuelve un codigo negativo y `*out` queda en NULL:
// `GEODA_ERR_INVALID_ARGUMENT` si `columns` es NULL o `n_columns` es 0, hay menos de 2 filas,
// `n_components` no esta entre 1 y `min(filas, n_columns)`, hay nulos o no finitos;
// `GEODA_ERR_LENGTH_MISMATCH` si las columnas miden distinto.
//
// # Safety
// `columns` debe apuntar a `n_columns` handles validos y `out` ser escribible y no NULL.
int32_t geoda_pca_fit(const struct GeodaColumn *const *columns,
                      size_t n_columns,
                      size_t n_components,
                      struct GeodaPca **out);

// Proyecta `n_columns` variables sobre los componentes: `(X - media) * componentes^T`. Devuelve una
// columna `Float64` nueva por `*out` con las `filas * k` entradas **por filas** (`k` componentes).
//
// `*out` se libera con `geoda_column_free`. Si falla, devuelve un codigo negativo y `*out` queda en
// NULL: `GEODA_ERR_INVALID_ARGUMENT` si `model` o `columns` son NULL, `n_columns` es 0 o hay nulos o no
// finitos; `GEODA_ERR_LENGTH_MISMATCH` si `n_columns` no es el numero de variables del ajuste.
//
// # Safety
// `model` debe ser un handle valido, `columns` apuntar a `n_columns` handles validos y `out`
// escribible y no NULL.
int32_t geoda_pca_transform(const struct GeodaPca *model,
                            const struct GeodaColumn *const *columns,
                            size_t n_columns,
                            struct GeodaColumn **out);

// Lee el modelo ajustado. Devuelve **cuatro columnas `Float64` nuevas**:
//
// - `*out_components`: `k * d` valores por filas (un componente por fila; `d` variables).
// - `*out_variance`: `k` varianzas explicadas (`s^2 / (n - 1)`), de mayor a menor.
// - `*out_ratio`: `k` razones de la varianza total.
// - `*out_mean`: `d` medias con las que se centra.
//
// Cada columna se libera con `geoda_column_free`. Si falla, devuelve un codigo negativo y los cuatro
// `out` quedan en NULL: `GEODA_ERR_INVALID_ARGUMENT` si `model` es NULL.
//
// # Safety
// `model` debe ser un handle valido y los cuatro `out` escribibles y no NULL.
int32_t geoda_pca_params(const struct GeodaPca *model,
                         struct GeodaColumn **out_components,
                         struct GeodaColumn **out_variance,
                         struct GeodaColumn **out_ratio,
                         struct GeodaColumn **out_mean);

// Libera un PCA. Acepta NULL (no hace nada). Despues de llamarla el handle ya no es valido; liberarlo
// dos veces es comportamiento indefinido.
//
// # Safety
// `model` debe ser NULL o un handle devuelto por `geoda_pca_fit` y no liberado aun.
void geoda_pca_free(struct GeodaPca *model);

// Crea una columna `Int32` desde `len` valores planos. `validity` es NULL (todo valido) o `len`
// bytes (1 = valido, 0 = nulo; el valor de un nulo se ignora). `*out` se libera con
// `geoda_column_free`; si falla queda en NULL.
//
// Errores: `GEODA_ERR_INVALID_ARGUMENT` si `values` (con `len > 0`) o `out` es NULL o `values`
// esta mal alineado, y `GEODA_ERR_OUT_OF_BOUNDS` si `len` bytes no caben en memoria.
//
// # Safety
// `values` debe apuntar a `len` valores legibles (puede ser NULL si `len` es 0), `validity` a `len`
// bytes legibles o ser NULL, y `out` ser escribible y no NULL.
int32_t geoda_column_from_i32(const int32_t *values,
                              size_t len,
                              const uint8_t *validity,
                              struct GeodaColumn **out);

// Como `geoda_column_from_i32`, para `Int64`.
//
// # Safety
// Los de `geoda_column_from_i32`.
int32_t geoda_column_from_i64(const int64_t *values,
                              size_t len,
                              const uint8_t *validity,
                              struct GeodaColumn **out);

// Como `geoda_column_from_i32`, para `Float64`. Un `NaN` es un valor valido, no un nulo.
//
// # Safety
// Los de `geoda_column_from_i32`.
int32_t geoda_column_from_f64(const double *values,
                              size_t len,
                              const uint8_t *validity,
                              struct GeodaColumn **out);

// Como `geoda_column_from_i32`, para `Bool`: cada valor es un byte (0 = falso, otro = verdadero).
//
// # Safety
// Los de `geoda_column_from_i32`.
int32_t geoda_column_from_bool(const uint8_t *values,
                               size_t len,
                               const uint8_t *validity,
                               struct GeodaColumn **out);

// Crea una columna `Utf8`. La fila `i` son los bytes `data[offsets[i] .. offsets[i + 1]]`:
// `offsets` tiene `len + 1` valores, el primero es 0, no decrecen y el ultimo es a lo sumo
// `data_len`. `validity` es NULL (todo valido) o `len` bytes; el contenido de un nulo se ignora.
// `*out` se libera con `geoda_column_free`; si falla queda en NULL.
//
// Errores: `GEODA_ERR_FFI` si los offsets no cumplen lo anterior o los bytes de una fila no son
// UTF-8; `GEODA_ERR_INVALID_ARGUMENT` si `offsets` o `out` es NULL, o `data` lo es con
// `data_len > 0`; `GEODA_ERR_OUT_OF_BOUNDS` si `len + 1` o `data_len` no caben en memoria.
//
// # Safety
// `data` debe apuntar a `data_len` bytes legibles (puede ser NULL si `data_len` es 0), `offsets` a
// `len + 1` valores legibles, `validity` a `len` bytes o ser NULL, y `out` escribible y no NULL.
int32_t geoda_column_from_utf8(const uint8_t *data,
                               size_t data_len,
                               const int32_t *offsets,
                               size_t len,
                               const uint8_t *validity,
                               struct GeodaColumn **out);

// Tipo de una columna: una de las constantes `GEODA_DTYPE_*`, o un codigo de error negativo si
// `column` es NULL.
//
// # Safety
// `column` debe ser NULL o un handle valido.
int32_t geoda_column_dtype(const struct GeodaColumn *column);

// Copia los valores de una columna al arreglo `out` de `capacity` elementos. Escribe `geoda_column_len`
// valores; el de un nulo es 0 (usa `geoda_column_copy_validity` para distinguirlo). Si `capacity` es
// menor que la longitud devuelve `GEODA_ERR_OUT_OF_BOUNDS` y **no escribe nada**.
//
// Errores: `GEODA_ERR_TYPE_MISMATCH` si la columna no es `Int32`; `GEODA_ERR_INVALID_ARGUMENT` si
// `column` u `out` (con longitud > 0) es NULL o `out` esta mal alineado.
//
// # Safety
// `column` debe ser un handle valido y `out` apuntar a `capacity` valores escribibles (puede ser
// NULL si la columna esta vacia).
int32_t geoda_column_copy_i32(const struct GeodaColumn *column,
                              int32_t *out,
                              size_t capacity);

// Como `geoda_column_copy_i32`, para `Int64`.
//
// # Safety
// Los de `geoda_column_copy_i32`.
int32_t geoda_column_copy_i64(const struct GeodaColumn *column, int64_t *out, size_t capacity);

// Como `geoda_column_copy_i32`, para `Float64`. Un nulo se copia como `0.0`; un `NaN` valido se
// conserva.
//
// # Safety
// Los de `geoda_column_copy_i32`.
int32_t geoda_column_copy_f64(const struct GeodaColumn *column, double *out, size_t capacity);

// Como `geoda_column_copy_i32`, para `Bool`: un byte por valor (0 o 1; un nulo es 0).
//
// # Safety
// Los de `geoda_column_copy_i32`.
int32_t geoda_column_copy_bool(const struct GeodaColumn *column, uint8_t *out, size_t capacity);

// Copia la validez de **cualquier** columna: un byte por fila, 1 si es valida y 0 si es nula. Si
// `capacity` es menor que la longitud devuelve `GEODA_ERR_OUT_OF_BOUNDS` y no escribe nada.
//
// # Safety
// `column` debe ser un handle valido y `out` apuntar a `capacity` bytes escribibles (puede ser NULL
// si la columna esta vacia).
int32_t geoda_column_copy_validity(const struct GeodaColumn *column, uint8_t *out, size_t capacity);

// Bytes totales de los textos de una columna `Utf8` (lo que hay que reservar para
// `geoda_column_copy_utf8`). Un nulo ocupa 0 bytes. Escribe el resultado en `*out`.
//
// Errores: `GEODA_ERR_TYPE_MISMATCH` si la columna no es `Utf8`; `GEODA_ERR_INVALID_ARGUMENT` si
// `column` u `out` es NULL.
//
// # Safety
// `column` debe ser un handle valido y `out` escribible y no NULL.
int32_t geoda_column_utf8_data_len(const struct GeodaColumn *column, size_t *out);

// Copia una columna `Utf8`: los bytes de todos los textos, uno tras otro, a `data` (de
// `data_capacity` bytes) y `len + 1` offsets a `offsets` (de `offsets_capacity` valores), de modo
// que la fila `i` es `data[offsets[i] .. offsets[i + 1]]`. Un nulo es una fila vacia (usa
// `geoda_column_copy_validity`). Si una capacidad no alcanza devuelve `GEODA_ERR_OUT_OF_BOUNDS` y
// no escribe nada.
//
// # Safety
// `column` debe ser un handle valido; `data` y `offsets` deben apuntar a sus capacidades de
// elementos escribibles (`data` puede ser NULL si no hay bytes que copiar).
int32_t geoda_column_copy_utf8(const struct GeodaColumn *column,
                               uint8_t *data,
                               size_t data_capacity,
                               int32_t *offsets,
                               size_t offsets_capacity);

// Reserva `size` bytes alineados a 8 y devuelve el puntero, o NULL si `size` es 0 o no hay memoria.
// Para quien embebe geoda sin `malloc` (WebAssembly). Se libera con `geoda_dealloc` y el mismo
// `size`.
uint8_t *geoda_alloc(size_t size);

// Libera memoria de `geoda_alloc`. Acepta NULL y `size` 0 (no hacen nada).
//
// # Safety
// `ptr` debe ser NULL o un puntero devuelto por `geoda_alloc(size)` con el mismo `size` y no
// liberado aun.
void geoda_dealloc(uint8_t *ptr, size_t size);

// Libera una columna. Acepta NULL (no hace nada). Despues de llamarla el handle
// ya no es valido; liberarlo dos veces es comportamiento indefinido.
//
// No devuelve codigo: si la liberacion entrara en panic, el mensaje queda en el
// buffer del hilo y puede leerse despues de una llamada sin relacion.
//
// # Safety
// `column` debe ser NULL o un handle devuelto por la API y no liberado aun.
void geoda_column_free(struct GeodaColumn *column);

// Longitud de la columna, o un codigo de error negativo si `column` es NULL.
//
// # Safety
// `column` debe ser NULL o un handle valido.
int64_t geoda_column_len(const struct GeodaColumn *column);

// Cantidad de nulos de la columna, o un codigo de error negativo si `column` es NULL.
//
// # Safety
// `column` debe ser NULL o un handle valido.
int64_t geoda_column_null_count(const struct GeodaColumn *column);

#ifdef __cplusplus
}  // extern "C"
#endif  // __cplusplus

#endif  /* GEODA_H */
