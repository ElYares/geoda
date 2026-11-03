# geoda (Python)

Binding de Python de geoda: PyO3 sobre `geoda-core`, `geoda-algo` y `geoda-stats`, construido con
maturin. Estadistica sobre columnas, con el nucleo en Rust.

## Instalar

Todavia no esta en PyPI. Se construye una wheel local y se instala con `pip`:

```bash
pip install maturin
bindings/python/build_wheel.sh /tmp/wheelhouse     # wheel de Linux, sin rutas de tu maquina
python -m venv /tmp/venv
/tmp/venv/bin/pip install /tmp/wheelhouse/geoda-*.whl pyarrow
/tmp/venv/bin/python examples/estadisticas.py      # resumen de una tabla, con comprobaciones
```

`pyarrow` es opcional: solo hace falta para pasar columnas Arrow. Con listas no se necesita.
Usa `build_wheel.sh` y no `maturin build` a secas: el script reescribe las rutas absolutas que
el compilador guarda en el binario.

## API

Todas las funciones numericas aceptan una **lista** (`None` = nulo) o cualquier objeto con el
protocolo Arrow PyCapsule (`__arrow_c_array__`: `pyarrow`, `polars`, `pandas` con Arrow...).
Los nulos se ignoran. Un `NaN` en los datos da `NaN` en el resultado, nunca se oculta.

| Funcion | Devuelve |
|---|---|
| `count(x)`, `sum(x)`, `min(x)`, `max(x)`, `mean(x)` | `count`: `int`; `sum`/`min`/`max`: `int` si `x` es una columna Arrow entera, `float` si no; `mean`: `float` |
| `var(x, ddof=1)`, `std(x, ddof=1)` | `float` |
| `median(x)`, `percentile(x, p)` | `float` (`p` en `[0, 100]`, interpolacion lineal tipo 7) |
| `corr(x, y)` | `float` en `[-1, 1]` (Pearson; las filas con nulo en cualquiera se descartan) |
| `histogram(x, bins, range=None)` | `dict` con `edges`, `counts`, `below`, `above`, `nulls`, `nans` |
| `group_by(claves).sum/count/min/max/mean/var/std(valores)` | `dict` con `keys` y `values` (listas; `None` es el grupo de claves nulas) |

- **`ddof` por defecto es 1** (varianza muestral, como R y pandas). **NumPy usa 0**:
  `np.var(a)` y `geoda.var(a)` no coinciden; usa `geoda.var(a, ddof=0)` para igualarlos.
- El histograma usa cubos `[a, b)` salvo el ultimo, cerrado. Lo que cae fuera de `range` se
  cuenta en `below` y `above`, no se descarta.
- Un tipo no soportado (texto, `decimal128`, listas anidadas) lanza `TypeError`; un argumento
  invalido (`ddof >= n`, sin valores validos, largos distintos) lanza `ValueError`.

## Limites conocidos

- **Solo Linux (x86_64)**: la wheel se construye en tu maquina y no se garantiza que sirva en otra distribucion; wheels `manylinux`, macOS, Windows y PyPI quedan para mas adelante.
- **Los datos se copian** al pasar una columna Arrow (no hay cero copia).
- Las **listas** de Python se convierten a `float`: `geoda.sum([1, 2, 3])` da `6.0`, y los
  enteros de magnitud mayor a 2^53 pierden precision. Con una columna Arrow entera, `sum`,
  `min` y `max` dan `int`.
- `histogram` y `group_by` devuelven listas dentro de un `dict`, no una tabla Arrow.
- `ChunkedArray` (`__arrow_c_stream__`) no se acepta; usa `combine_chunks()`.

## Desarrollo

```bash
cargo build -p geoda-capi                 # libgeoda, para el runner de spec
cd bindings/python
uv venv .venv && uv pip install maturin pyarrow numpy
VIRTUAL_ENV=$PWD/.venv .venv/bin/maturin develop
.venv/bin/python -c "import geoda; print(geoda.version())"
.venv/bin/python -m unittest discover -s tests -v
```

## Tests

- `tests/test_spec.py`: runner de `spec/` por `ctypes` sobre `libgeoda` (solo stdlib).
  Hace lo mismo que `crates/capi/tests/spec.rs`.
- `tests/test_interop_pyarrow.py`: pyarrow como oraculo de Arrow, en ambos sentidos.
  Se salta si `pyarrow` no esta instalado; en CI debe correr.
- `tests/test_arrow_input.py`, `test_stats.py`, `test_tables.py`: la API de arriba, con listas y
  con columnas Arrow (los de Arrow necesitan `pyarrow` y `numpy`).
- `tests/capi.py`: declaraciones `ctypes` de la C-API y de las structs de Arrow.
  `GEODA_LIB` apunta a otra `libgeoda` si no esta en `target/{debug,release}`.
