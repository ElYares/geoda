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

Cada runner recorre `cases/**/*.json`. Hoy hay uno, en Rust
(`crates/capi/tests/spec.rs`, corre con `cargo test -p geoda-capi spec`); HU-008
anade el de Python.

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

### `expect`

Se comprueban solo las claves presentes:

- `dtype`, `values`: sobre la columna que **sale** de la capi (exportada).
- `len`, `null_count`: sobre el handle resultante.
- `error`: la operacion debe fallar con esa categoria; si tiene exito, o falla con otra,
  el caso falla. Las categorias son el sufijo de `GEODA_ERR_*` en `geoda.h`:
  `INVALID_ARGUMENT`, `TYPE_MISMATCH`, `LENGTH_MISMATCH`, `OUT_OF_BOUNDS`,
  `UNSUPPORTED`, `ALLOC`, `FFI`, `PANIC`, `INTERNAL`.

`error` no se mezcla con las demas claves.

## Reglas

- Los casos no llevan datos grandes; lo voluminoso va a `bench/`.
- Cada funcion nueva de la C-API anade al menos un caso aqui.
- Si `spec/` crece a miles de casos, se genera con un script; no antes.
