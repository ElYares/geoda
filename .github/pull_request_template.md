## Qué cambia y por qué

<!-- Una o dos frases. Enlaza el issue si lo hay. -->

## Comprobación

- [ ] `cargo fmt --all --check`
- [ ] `cargo clippy --workspace --exclude geoda-py --all-targets -- -D warnings`
- [ ] `cargo test --workspace --exclude geoda-py`
- [ ] Una función nueva de la capi trae su caso en `spec/`
- [ ] Si toqué la capi: `tests/c/run.sh` y `abi/check.sh` pasan (y, si añadí una función, subí `ABI_MINOR` y corrí `abi/check.sh --update`)
- [ ] Si toqué un binding: corrí sus tests (ver `README.md`)
- [ ] Todo `unsafe` nuevo lleva su comentario `SAFETY`
- [ ] Identificadores en inglés; comentarios, docs y mensajes de error en español
- [ ] `git status` limpio de artefactos (`build/`, `target/`)
