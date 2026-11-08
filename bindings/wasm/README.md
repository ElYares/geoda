# geoda para WebAssembly

La C-API (ABI 1.x, [`docs/abi.md`](../../docs/abi.md)) compilada a **`wasm32-unknown-unknown`** y un
cargador de JavaScript (`geoda.mjs`, con tipos en `geoda.d.ts`). Sin `wasm-bindgen`, sin WASI y **sin hilos**:
el módulo no importa nada y exporta las 75 funciones `geoda_*` más `memory`.

```js
import fs from 'node:fs';
import { Geoda } from './geoda.mjs';

const geoda = await Geoda.load(fs.readFileSync('geoda.wasm'));     // navegador: Geoda.load(fetch('geoda.wasm'))
const column = geoda.Column.fromFloat64(Float64Array.of(1.5, 99, 3.5), Uint8Array.of(1, 0, 1)); // el 99 es nulo
geoda.mean(column);                                                // 2.5
geoda.solve([[2, 1], [1, 3]], [3, 5]);                             // [0.8, 1.4]
geoda.kmeans([[0, 0, 10, 10], [0, 1, 10, 11]], 2, { seed: 1n }).labels;   // [0, 0, 1, 1]
column.free();                                                     // idempotente; el GC libera lo que se olvide
```

La API es la misma que la del [binding de Node](../node/README.md) (columnas con typed arrays, `BigInt` para
`int64`, errores con `code` y `category`), salvo que las funciones son métodos de la instancia (`geoda.mean`)
y `Column` también (`geoda.Column`): cada instancia tiene su propia memoria.

## Construir y probar

```bash
bindings/wasm/build.sh        # cargo build -p geoda-capi --target wasm32-unknown-unknown --release -> geoda.wasm
cd bindings/wasm && npm test  # pruebas propias + el subconjunto de spec/ contra el .wasm
```

`build.sh` usa el `cargo` del PATH si tiene el objetivo `wasm32-unknown-unknown`
(`rustup target add wasm32-unknown-unknown`). Si no, busca una toolchain upstream aislada en
`$GEODA_WASM_TOOLCHAIN` o `~/.local/share/geoda-wasm/toolchain`: el `std` de wasm32 de upstream no es
compatible con el `rustc` de distribuciones como Arch.

## Qué cambia respecto a la versión nativa

- **Un solo hilo.** Pedir `workers > 1` a la C-API da `GEODA_ERR_UNSUPPORTED` (y no un `panic` al lanzar
  hilos); el binding siempre pasa 1. Los resultados no dependen de `workers`.
- **Sin archivos.** `geoda_core::file` (columnas en disco) compila pero falla en ejecución.
- **Sin recuperación de `panic`.** En `wasm32-unknown-unknown` un `panic` aborta el módulo (`unreachable`) en
  vez de volverse `GEODA_ERR_PANIC`. Las funciones validan sus entradas y devuelven códigos de error; el
  `panic` es un fallo de geoda, no una condición esperada.
- **Memoria.** `geoda_alloc`/`geoda_dealloc` reservan en la memoria lineal del módulo, que puede crecer en
  cualquier llamada: por eso las vistas (`Float64Array`...) se crean justo antes de usarlas.

## Pruebas

`test/geoda.test.mjs` comprueba que el `.wasm` **exporta exactamente** `abi/symbols.txt`, que no importa nada,
que `workers > 1` es `UNSUPPORTED`, que crear y liberar columnas no deja fugas y que 2 millones de filas
(memoria que crece) funcionan. `test/spec.test.mjs` corre los casos de `spec/cases/` que soporta la
superficie mínima con el runner compartido de [`bindings/spec-runner`](../spec-runner/run.mjs).
