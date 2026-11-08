"""geoda: analisis y mineria de datos con nucleo en Rust."""

from . import _geoda
from ._geoda import (
    Graph,
    column_len,
    corr,
    count,
    dedupe,
    edit_distance,
    histogram,
    huffman_decode,
    huffman_encode,
    lis,
    max,
    mean,
    median,
    min,
    percentile,
    std,
    sum,
    var,
    version,
)


class GroupBy:
    """Agrupa por los valores de una columna clave (`geoda.group_by(keys)`).

    Cada metodo calcula un agregado de `values` en cada grupo y devuelve un `dict` con dos
    listas del mismo largo: `keys` (en orden de primera aparicion; `None` es el grupo de
    claves nulas) y `values`. Las claves nulas forman su propio grupo; todos los `NaN` son un
    grupo y `-0.0` y `0.0` son grupos distintos.

    `workers` (por defecto 1) reparte entre hilos los bloques de cada grupo: el resultado no
    depende de el, pero un grupo de un solo bloque (1024 valores) no lanza hilos.
    """

    def __init__(self, keys):
        self._keys = keys

    def _run(self, agg, values, ddof=1, workers=1):
        keys, results = _geoda.group_aggregate(self._keys, values, agg, ddof, workers)
        return {"keys": keys, "values": results}

    def count(self, values, workers=1):
        """Valores no nulos de `values` en cada grupo."""
        return self._run("count", values, workers=workers)

    def sum(self, values, workers=1):
        """Suma por grupo (`int` si `values` es una columna Arrow entera)."""
        return self._run("sum", values, workers=workers)

    def min(self, values, workers=1):
        """Minimo por grupo."""
        return self._run("min", values, workers=workers)

    def max(self, values, workers=1):
        """Maximo por grupo."""
        return self._run("max", values, workers=workers)

    def mean(self, values, workers=1):
        """Media por grupo."""
        return self._run("mean", values, workers=workers)

    def var(self, values, ddof=1, workers=1):
        """Varianza por grupo con `ddof` grados de libertad restados (por defecto 1)."""
        return self._run("var", values, ddof, workers)

    def std(self, values, ddof=1, workers=1):
        """Desviacion estandar por grupo con `ddof` grados de libertad restados."""
        return self._run("std", values, ddof, workers)


def group_by(keys):
    """Agrupa por los valores de `keys` (una columna Arrow o una lista de numeros)."""
    return GroupBy(keys)


__all__ = [
    "Graph",
    "GroupBy",
    "column_len",
    "corr",
    "count",
    "group_by",
    "histogram",
    "max",
    "mean",
    "median",
    "min",
    "percentile",
    "std",
    "sum",
    "var",
    "version",
]
