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
| `input` | si | la columna de entrada: `dtype` y `values` |
| `op` | si | la operacion (ver abajo) |
| `input2` | solo `corr` | la segunda columna, con el mismo formato que `input` |
| `args` | segun `op` | parametros de la operacion |
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

### `expect`

Se comprueban solo las claves presentes:

- `dtype`, `values`: sobre la columna que **sale** de la capi (exportada).
- `len`, `null_count`: sobre el handle resultante.
- `value`: el numero que devuelve la operacion. Un `float64` se compara bit a bit y `NaN` es igual a `NaN`, como el resto de los flotantes; se escribe con los mismos valores especiales. Un resultado entero (`count`, y `sum`/`min`/`max` sobre enteros) se escribe como entero de JSON y se compara exacto.
- `error`: la operacion debe fallar con esa categoria; si tiene exito, o falla con otra,
  el caso falla. Las categorias son el sufijo de `GEODA_ERR_*` en `geoda.h`:
  `INVALID_ARGUMENT`, `TYPE_MISMATCH`, `LENGTH_MISMATCH`, `OUT_OF_BOUNDS`,
  `UNSUPPORTED`, `ALLOC`, `FFI`, `PANIC`, `INTERNAL`.

`error` no se mezcla con las demas claves.

## Reglas

- Los casos no llevan datos grandes; lo voluminoso va a `bench/`.
- Cada funcion nueva de la C-API anade al menos un caso aqui.
- Si `spec/` crece a miles de casos, se genera con un script; no antes.
