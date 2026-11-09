# Versiones de geoda

## Dos números distintos

| Número | Dónde vive | Qué dice |
|---|---|---|
| **Versión del paquete** | `Cargo.toml` (workspace), `package.json` de Node y WASM; Python la lee del `Cargo.toml` (`dynamic`) | qué lanzamiento es |
| **Versión de la ABI** | `abi/VERSION` y `ABI_MAJOR`/`ABI_MINOR` en `crates/capi/src/lib.rs` | qué promete la frontera C ([`abi.md`](abi.md)) |

No se mueven juntos. La ABI solo sube cuando cambia la C-API; el paquete sube en cada lanzamiento. Un
paquete `0.3.0` puede seguir con la ABI `1.0`.

## SemVer 0.x

Mientras el paquete sea `0.y.z`:

- **`0.y.z` → `0.y.(z+1)`**: correcciones. No rompen nada.
- **`0.y.z` → `0.(y+1).0`**: pueden **romper** la API de Rust, de Python, de Java, de C++, de Node o de WASM.
  Cada ruptura se anota en el CHANGELOG bajo «Cambiado» o «Eliminado».
- La `1.0.0` del paquete llegará cuando las APIs de los bindings dejen de moverse.

## Qué es estable en la 0.1.0

- **La C-API (ABI 1.0):** nombres, firmas, constantes y reglas de la frontera, según [`abi.md`](abi.md).
  Añadir funciones sube el menor de la ABI; romper algo exige un mayor nuevo.
- **El significado de cada función** tal como lo fija `spec/`.

## Qué no lo es

- Los crates de Rust (`geoda-core`, `geoda-algo`, `geoda-stats`): sus APIs pueden cambiar en cada `0.minor`.
- Las APIs de los bindings (Python, C++, Java, Node, WASM): pueden cambiar en cada `0.minor`, aunque la ABI
  no cambie.
- Los mensajes de error, el orden de resultados que la spec no fija y el rendimiento.

## Cómo se sube la versión

`release/bump.sh X.Y.Z` cambia a la vez el `Cargo.toml` del workspace, el `Cargo.lock` y los `package.json`
de Node y WASM; Python la toma del `Cargo.toml`. La ABI no se toca con ese script: sigue las reglas de
[`abi.md`](abi.md). Los pasos completos de un lanzamiento están en [`RELEASING.md`](../RELEASING.md).
