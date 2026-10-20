/* geoda C-API (v0). Generado con cbindgen: no editar a mano. */

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

#ifdef __cplusplus
extern "C" {
#endif // __cplusplus

// Version de la ABI como `major << 16 | minor`. Mientras la ABI no se congele
// (fase 8) cualquier cambio puede romperla, incluso con el mismo `major`.
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
