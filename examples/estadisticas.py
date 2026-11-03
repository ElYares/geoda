"""Resumen estadistico de una tabla con geoda y pyarrow (HU-028).

Uso, en un entorno con la wheel de geoda y `pyarrow` instalados:

    python examples/estadisticas.py

Imprime el resumen y comprueba cada resultado contra el valor calculado a mano: si algo
discrepa, termina con un error. Los datos son pequenos a proposito para que se pueda
verificar de cabeza.
"""

import math

import geoda
import pyarrow as pa

# Una tabla con un grupo, una columna entera y una de flotantes (`y = 2 * x`).
tabla = pa.table(
    {
        "grupo": ["a", "b", "a", "b", "a", "b"],
        "x": pa.array([1, 2, 3, 4, 5, 6], pa.int64()),
        "y": pa.array([2.0, 4.0, 6.0, 8.0, 10.0, 12.0], pa.float64()),
    }
)
grupo, x, y = (tabla.column(nombre).combine_chunks() for nombre in ("grupo", "x", "y"))

resumen = {
    "version": geoda.version(),
    "media de x": geoda.mean(x),
    # `ddof=1` por defecto (varianza muestral, como R y pandas); NumPy usa `ddof=0`.
    "varianza de x": geoda.var(x),
    "varianza de x (ddof=0)": geoda.var(x, ddof=0),
    "mediana de x": geoda.median(x),
    "correlacion x-y": geoda.corr(x, y),
    "histograma de x": geoda.histogram(x, bins=3),
    "suma de x por grupo": geoda.group_by(grupo).sum(x),
}
for nombre, valor in resumen.items():
    print(f"{nombre}: {valor}")

# Cada resultado se compara con el calculado a mano.
assert resumen["media de x"] == 3.5
assert resumen["varianza de x"] == 3.5  # 17.5 / (6 - 1)
assert math.isclose(resumen["varianza de x (ddof=0)"], 35 / 12)
assert resumen["mediana de x"] == 3.5
assert math.isclose(resumen["correlacion x-y"], 1.0, rel_tol=0, abs_tol=1e-12)
histograma = resumen["histograma de x"]
assert histograma["counts"] == [2, 2, 2]
assert histograma["edges"][0] == 1.0 and histograma["edges"][-1] == 6.0
assert (histograma["below"], histograma["above"], histograma["nulls"], histograma["nans"]) == (0, 0, 0, 0)
assert resumen["suma de x por grupo"] == {"keys": ["a", "b"], "values": [9, 12]}
print("ok: todos los resultados coinciden")
