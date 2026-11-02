"""geoda: analisis y mineria de datos con nucleo en Rust."""

from . import _geoda
from ._geoda import (
    column_len,
    corr,
    count,
    histogram,
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
    """

    def __init__(self, keys):
        self._keys = keys

    def _run(self, agg, values, ddof=1):
        keys, results = _geoda.group_aggregate(self._keys, values, agg, ddof)
        return {"keys": keys, "values": results}

    def count(self, values):
        """Valores no nulos de `values` en cada grupo."""
        return self._run("count", values)

    def sum(self, values):
        """Suma por grupo (`int` si `values` es una columna Arrow entera)."""
        return self._run("sum", values)

    def min(self, values):
        """Minimo por grupo."""
        return self._run("min", values)

    def max(self, values):
        """Maximo por grupo."""
        return self._run("max", values)

    def mean(self, values):
        """Media por grupo."""
        return self._run("mean", values)

    def var(self, values, ddof=1):
        """Varianza por grupo con `ddof` grados de libertad restados (por defecto 1)."""
        return self._run("var", values, ddof)

    def std(self, values, ddof=1):
        """Desviacion estandar por grupo con `ddof` grados de libertad restados."""
        return self._run("std", values, ddof)


def group_by(keys):
    """Agrupa por los valores de `keys` (una columna Arrow o una lista de numeros)."""
    return GroupBy(keys)


__all__ = [
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
