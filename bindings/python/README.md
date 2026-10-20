# geoda (Python)

Binding de Python de geoda: PyO3 sobre `geoda-core`, construido con maturin.
Fase 0: solo `geoda.version()` y `geoda.column_len(lista)`.

## Desarrollo

```bash
cargo build -p geoda-capi                 # libgeoda, para el runner de spec
cd bindings/python
uv venv .venv && uv pip install maturin pyarrow
VIRTUAL_ENV=$PWD/.venv .venv/bin/maturin develop
.venv/bin/python -c "import geoda; print(geoda.version())"
.venv/bin/python -m unittest discover -s tests -v
```

## Tests

- `tests/test_spec.py`: runner de `spec/` por `ctypes` sobre `libgeoda` (solo stdlib).
  Hace lo mismo que `crates/capi/tests/spec.rs`.
- `tests/test_interop_pyarrow.py`: pyarrow como oraculo de Arrow, en ambos sentidos.
  Se salta si `pyarrow` no esta instalado; en CI debe correr.
- `tests/capi.py`: declaraciones `ctypes` de la C-API y de las structs de Arrow.
  `GEODA_LIB` apunta a otra `libgeoda` si no esta en `target/{debug,release}`.
