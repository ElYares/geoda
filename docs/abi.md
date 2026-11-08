# ABI de geoda (C-API) · versión 1.0

La frontera de geoda es la biblioteca `libgeoda` y su cabecera
[`crates/capi/include/geoda.h`](../crates/capi/include/geoda.h). Todos los bindings (Python, C++, Java,
Node, WASM) hablan con ella; nada más cruza la frontera.

Desde la **1.0** está congelada (decisión 015, O3). Esta página dice qué promete, qué no y cómo se
cambia.

## Versión

`geoda_abi_version()` devuelve `major << 16 | minor`. La 1.0 es `0x00010000` (65536).

| Cambio | Versión |
|---|---|
| Añadir una función, una constante o un código de error nuevo | sube el **menor** |
| Quitar o renombrar una función, cambiar una firma, el significado de una constante o de un código de error | sube el **mayor** |

Un binding compilado contra `1.x` funciona con cualquier `1.y` con `y >= x`.

## Qué es estable

- Los **nombres y firmas** de las 75 funciones `geoda_*` (la lista exacta está en
  [`abi/symbols.txt`](../abi/symbols.txt)).
- Las **constantes** de la cabecera: `GEODA_OK` y los `GEODA_ERR_*` (0 y -1 a -9), `GEODA_AGG_*`,
  `GEODA_DTYPE_*`.
- Las **reglas** de la frontera:
  - toda función falible devuelve un código (`GEODA_OK` o uno negativo);
  - si falla, **todos** los `out` de la llamada quedan en NULL y los escalares no se escriben;
  - un handle devuelto se libera con su `*_free` (acepta NULL); liberarlo dos veces es comportamiento
    indefinido;
  - una matriz de entrada es una lista de columnas (`const GeodaColumn *const *` + `n`) y una de salida,
    una columna `Float64` plana por filas (decisiones 013 y 014);
  - los arreglos planos llevan su capacidad y no se escribe nada si no alcanza.
- El **significado** de cada función tal como lo documenta la cabecera y fija `spec/`.

## Qué no lo es

- El **diseño interno** de los handles (`GeodaColumn`, `GeodaGraph`, `GeodaTree`, `GeodaRegression`,
  `GeodaPca` son opacos).
- Los **mensajes** de `geoda_last_error_message` (sirven para leer, no para comparar); la
  **categoría** es el código.
- El orden de los resultados que la spec no fija, y el rendimiento.

## Cómo se vigila

`abi/check.sh` compila `libgeoda.so`, saca los símbolos `geoda_*` con `nm -D` y los compara con
`abi/symbols.txt`; también comprueba que `abi/VERSION` coincide con `ABI_MAJOR.ABI_MINOR` de
`crates/capi/src/lib.rs`. El job `capi` de CI lo ejecuta, y `tests/c/run.sh` comprueba además que la
cabecera no tiene diff.

Para cambiar la ABI **a propósito**:

1. Sube `ABI_MINOR` (o `ABI_MAJOR` y pon `ABI_MINOR` en 0) en `crates/capi/src/lib.rs`.
2. Pon la misma versión en `abi/VERSION`.
3. Corre `abi/check.sh --update` y `tests/c/run.sh` (regenera la cabecera con `cbindgen`).
4. Actualiza la prueba que fija `geoda_abi_version()` (`tests/c/smoke.c`, `crates/capi`, Python).
