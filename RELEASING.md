# Publicar geoda

**Nada de esto se ha hecho ni se hace solo.** Publicar es irreversible (PyPI, npm, Maven Central y crates.io no
dejan reutilizar una versión borrada), vuelve público un repositorio que hoy es privado y necesita credenciales
tuyas. Por eso (decisión 016, P3) el proyecto trae los paquetes **construidos y verificados**, con **seguros**
que hay que quitar a propósito, y esta guía.

## Construir todos los paquetes (sin publicar)

```bash
release/build.sh dist        # o, en GitHub: Actions → «Release» → Run workflow (solo construye y sube artefactos)
```

Deja en `dist/`: la wheel de Python, `elyares-geoda-0.0.0.tgz` y `elyares-geoda-wasm-0.0.0.tgz` (npm), `geoda.jar`, `geoda.wasm`,
`geoda-cpp.tar.gz` (cabeceras + CMake + docs de la ABI), `libgeoda.so` y `SHA256SUMS`. El flujo
`.github/workflows/release.yml` es manual (`workflow_dispatch`), no usa `secrets.*` y no tiene ningún paso de
publicación.

## Antes de publicar nada

1. Decide si el repositorio pasa a público (PyPI/npm/crates.io lo exponen en la ficha) y revisa la licencia
   (`MIT OR Apache-2.0`).
2. Sube las versiones: todo está en `0.0.0` (`Cargo.toml` del workspace, `package.json` de Node y WASM).
   Sigue [`docs/abi.md`](docs/abi.md): la versión del paquete y la de la ABI son cosas distintas.
3. `git status` limpio, CI en verde en `main` y `abi/check.sh` sin cambios.
4. Corre `release/build.sh` y comprueba `SHA256SUMS`.

## Nombres (comprobados el 2026-11-09, solo lectura)

| Registro | Nombre | Estado |
|---|---|---|
| PyPI | `geoda` | libre |
| npm | `geoda` | libre, pero se usa **`@elyares/geoda`** |
| npm | `geoda-wasm` | **ocupado** (otra persona, v0.0.5): se usa **`@elyares/geoda-wasm`** |
| crates.io | `geoda-core`, `geoda-algo`, `geoda-stats`, `geoda-capi` | libres |
| Maven Central | `io.geoda` | pide demostrar que el dominio `geoda.io` es tuyo; sin él, `io.github.elyares` |

Vuelve a comprobarlos justo antes de publicar: un nombre libre hoy puede no estarlo mañana. El scope `@elyares` de
npm tiene que existir en tu cuenta.

## Los seguros (quitarlos es el acto de publicar)

| Dónde | Seguro | Qué hacer |
|---|---|---|
| `Cargo.toml` (workspace) | `publish = false` | quitarlo y añadir versión a las dependencias `path` de cada crate |
| `bindings/python/pyproject.toml` | clasificador `Private :: Do Not Upload` (PyPI rechaza subidas con él) | quitarlo |
| `bindings/node/package.json`, `bindings/wasm/package.json` | `"private": true` | quitarlo |

## PyPI (wheel de Python)

- **Credencial:** un token de API de PyPI limitado al proyecto (mejor: *Trusted Publishing* con OIDC desde
  GitHub Actions, sin token guardado). Hazlo primero en **TestPyPI**.
- **Ojo:** `release/build.sh` construye la wheel con `maturin` en la máquina que lo corre y sale etiquetada
  `manylinux_2_34` (solo sirve donde glibc ≥ 2.34). Para llegar a más sistemas, construye en un contenedor
  `manylinux` antiguo: `maturin build --release --manylinux 2_28` con la imagen `ghcr.io/pyo3/maturin`, o
  `PyO3/maturin-action`. macOS y Windows no están cubiertos.
- **Comandos:** `maturin sdist`, `twine check dist/*`, `twine upload --repository testpypi dist/*`; luego
  `twine upload dist/*`.
- **No se deshace:** un nombre de archivo subido no se puede volver a usar aunque lo borres.

## npm (Node y WASM)

- **Credencial:** `npm login` o un token granular con permiso de publicar en `@elyares`.
- **Ojo (Node):** el paquete lleva el addon en C y lo compila al instalar con `node-gyp`; **`libgeoda` no va
  dentro**. Antes de publicar de verdad hay que decidir cómo se distribuye (prebuilds por plataforma o
  dependencia de una `libgeoda` instalada). El paquete WASM no tiene ese problema: lleva su `geoda.wasm`.
- **Comandos:** `cd bindings/wasm && npm publish --access public --dry-run`, y sin `--dry-run` al final. Con
  `--provenance` desde Actions si se publica ahí. `npm pack --dry-run` lista los archivos.
- **Ojo con `--dry-run`:** en npm 11 no aplica `private`; no lo uses para comprobar el seguro.
- **No se deshace:** `npm unpublish` solo funciona en las primeras 72 horas y el número de versión no se reutiliza.

## Maven Central (jar de Java)

- **Falta:** un `pom.xml` (hoy solo hay `javac` + `jar`), los jar de fuentes y de Javadoc, y la firma con GPG.
- **Credencial:** cuenta en el Central Portal de Sonatype con el *namespace* verificado, y una clave GPG.
- **No se deshace:** Central no borra versiones.
- La biblioteca nativa tampoco va en el jar: quien lo use necesita `libgeoda` (`-Dgeoda.lib=` o `GEODA_LIB`).

## crates.io (crates de Rust)

- **Falta:** versiones en las dependencias `path`, un README por crate y decidir si `geoda-capi` (cdylib) y
  `geoda-py` se publican (`geoda-bench` no).
- **Credencial:** token de crates.io (`cargo login`).
- **Comandos:** `cargo publish --dry-run -p geoda-core`, y así en orden de dependencias: core → stats → algo → capi.
- **No se deshace:** `cargo yank` oculta una versión pero no la borra.

## C y C++ (sin registro)

`geoda-cpp.tar.gz` y `libgeoda.so` se pueden adjuntar a un *release* de GitHub
(`gh release create vX.Y.Z dist/geoda-cpp.tar.gz dist/libgeoda.so dist/SHA256SUMS`).
En un repositorio privado solo lo ven los colaboradores.
