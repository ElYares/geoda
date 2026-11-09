# Cómo contribuir a geoda

Gracias por el interés. Esta guía dice cómo construir, probar y proponer cambios. Para reportar una
vulnerabilidad **no abras un issue**: sigue [`SECURITY.md`](SECURITY.md). Al participar aceptas el
[Código de conducta](CODE_OF_CONDUCT.md).

## Antes de empezar

- Para algo pequeño (un arreglo, una errata), abre el pull request directamente.
- Para algo grande (una función nueva, un cambio de diseño), abre antes un issue con la plantilla
  «Función nueva» y espera una respuesta. Así no escribes código que no se pueda aceptar.
- Las contribuciones se publican bajo la doble licencia del proyecto, [MIT](LICENSE-MIT) o
  [Apache-2.0](LICENSE-APACHE), a elección de quien use el código.

## Construir y probar

Los comandos de cada binding están en el [`README.md`](README.md#construir-y-probar). Lo mínimo antes de
abrir un pull request es lo mismo que corre la CI:

```bash
cargo fmt --all --check
cargo clippy --workspace --exclude geoda-py --all-targets -- -D warnings
cargo test --workspace --exclude geoda-py
```

`cargo clippy` a secas no falla con los lints pedantic; la CI usa `-D warnings`. La CI usa el Rust
`stable`, que puede traer lints nuevos que tu versión local no conoce: mira los checks del pull request.

Si tocas la C-API o un binding, corre además lo que le corresponde:

| Si tocas | Corre |
|---|---|
| `crates/capi` | `tests/c/run.sh` y `abi/check.sh` |
| `bindings/python` | [`bindings/python/README.md`](bindings/python/README.md) |
| `bindings/java` | `bindings/java/test.sh` (JDK 22 o superior) |
| `bindings/cpp` | CMake y `ctest`, como en el README |
| `bindings/node` | `npm test` en `bindings/node` |
| `bindings/wasm` | `bindings/wasm/build.sh` y `npm test` en `bindings/wasm` |

## Reglas que importan

### La verdad vive en `spec/`

Los casos de [`spec/`](spec/README.md) son JSON que corren los runners de Rust y de Python (y un subconjunto,
el de JavaScript). **Si un binding discrepa de un caso, el binding está mal.** No cambies el valor esperado
de un caso para que pase un binding; si crees que el caso está mal, explícalo en el pull request.

- Toda función nueva de la C-API lleva al menos un caso en `spec/`.
- Los valores esperados de álgebra lineal se calculan con un script, no a mano, y usan `expect.tolerance`.
- Al generar casos grandes, compacta las listas: un valor por línea llena el diff de miles de líneas.

### La ABI está congelada (1.0)

Cambiar o quitar un símbolo o una firma de la C-API rompe la CI (`abi/check.sh`). Añadir una función es
compatible, pero exige subir `ABI_MINOR` en `crates/capi/src/lib.rs` y en `abi/VERSION`, y correr
`abi/check.sh --update`. Los detalles están en [`docs/abi.md`](docs/abi.md).

Cualquier cambio en un comentario de documentación de la capi obliga a regenerar `crates/capi/include/geoda.h`
con `cbindgen` (la versión que fija la CI); `tests/c/run.sh` te avisa si falta.

### Código `unsafe`

Va solo en `memory/`, en `interop/arrow_c.rs`, en la capi, en el runner de Rust de la spec y en los bindings.
Todo cambio ahí necesita un comentario `SAFETY` que diga qué se promete y por qué se cumple. Dos cosas que ya
nos fallaron:

- Un tamaño que viene de C se calcula con `checked_*` y no se pasa a `from_raw_parts` si excede `isize::MAX`.
- Nunca uses un número que viene de C como capacidad inicial (`with_capacity`).

### Estilo del código

- Los identificadores (variables, funciones, tipos, constantes, archivos) van en inglés.
- Los comentarios, la documentación y los mensajes de error van en español. Si tocas una línea con nombres
  en otro idioma, renómbrala; no hagas barridos de renombrado aparte.
- `crates/core` no tiene dependencias, y la CI lo comprueba. No añadas una.
- En los tests, nombra las variables con palabras: clippy pedantic rechaza los nombres de una letra repetidos.
  Para comprobar que algo está vacío usa `assert_eq!(x.len(), 0)`.

## Commits y pull requests

- Usa [Conventional Commits](https://www.conventionalcommits.org/es/): `feat:`, `fix:`, `docs:`, `refactor:`,
  `test:`, `chore:`. El mensaje va en español y explica el porqué, no solo el qué.
- Un pull request, un cambio. Separa los refactors de las funciones nuevas.
- Trabaja en una rama (`feat/...`, `fix/...`) y abre el pull request contra `main`. Se integra con *rebase*.
- Antes de pedir revisión, mira `git status`: que no se cuelen artefactos de `build/` ni de `target/`.
- La plantilla del pull request trae la lista de comprobación; márcala de verdad.

## Cómo se revisa

Quien revisa lee el diff completo. Si algo no se entiende, se pregunta, y un cambio sin test o sin caso de
spec se devuelve. Una respuesta tarda unos días; si pasa más de una semana, vuelve a escribir en el pull request.
