"""Pruebas de `geoda.Graph` (fase 4): constructor, errores y los casos de `spec/cases/graph/`.

Los casos de la spec se ejecutan aqui por el binding nativo (sin pasar por la capi), asi que
los dos caminos tienen que dar los mismos valores que fija la spec.
"""

import json
import math
import unittest
from pathlib import Path

import geoda

CASES = sorted((Path(__file__).resolve().parents[3] / "spec" / "cases" / "graph").glob("*.json"))
# Categoria de la spec -> excepcion de Python.
EXCEPTIONS = {
    "INVALID_ARGUMENT": ValueError,
    "OUT_OF_BOUNDS": ValueError,
    "LENGTH_MISMATCH": ValueError,
    "TYPE_MISMATCH": TypeError,
}
# Un nulo en las aristas: en Python un `None` no es un entero, asi que es `TypeError` antes de
# llegar al nucleo (la capi lo rechaza con INVALID_ARGUMENT).
CASE_EXCEPTIONS = {"graph/error_graph_null_node": TypeError}


def special(value):
    """Flotante de la spec: numero o una de las cadenas especiales."""
    return {"NaN": math.nan, "Infinity": math.inf, "-Infinity": -math.inf}.get(value, value)


def same(actual, expected):
    if isinstance(expected, float) or isinstance(actual, float):
        expected, actual = special(expected), special(actual)
        return (math.isnan(expected) and math.isnan(actual)) or math.copysign(
            1, expected
        ) == math.copysign(1, actual) and expected == actual
    return actual == expected


class SpecCases(unittest.TestCase):
    def test_there_are_cases(self):
        self.assertGreaterEqual(len(CASES), 10)

    def test_all_graph_cases_pass(self):
        for path in CASES:
            with self.subTest(case=path.name):
                run(json.loads(path.read_text()))


def run(case):
    spec, op, expect = case["graph"], case["op"], case["expect"]
    weights = spec.get("weights")
    if weights is not None:
        weights = [special(w) for w in weights]
    args = case.get("args", {})
    try:
        graph = geoda.Graph(spec["n"], spec["edges"], spec["directed"], weights)
        result = {
            "graph_bfs": lambda: graph.bfs(args.get("source")),
            "graph_components": graph.components,
            "graph_topological_order": lambda: {"order": graph.topological_order()},
            "graph_dijkstra": lambda: graph.dijkstra(args.get("source")),
            "graph_mst": graph.mst,
        }[op]()
    except (ValueError, TypeError) as e:
        wanted = CASE_EXCEPTIONS.get(case["id"]) or EXCEPTIONS.get(expect.get("error"))
        assert wanted is not None and isinstance(e, wanted), f"fallo inesperado: {e!r} ({expect})"
        return
    assert "error" not in expect, f"esperaba {expect['error']}, pero tuvo exito"
    # `expect.columns` sigue el orden de columnas de la capi; aqui son las claves del dict.
    keys = {
        "graph_bfs": ["order", "distance", "parent"],
        "graph_components": ["ids"],
        "graph_topological_order": ["order"],
        "graph_dijkstra": ["distance", "parent"],
        "graph_mst": ["edges"],
    }[op]
    for key, column in zip(keys, expect["columns"]):
        got = result[key]
        assert len(got) == len(column["values"]), f"{key}: {got} != {column['values']}"
        assert all(same(g, w) for g, w in zip(got, column["values"])), f"{key}: {got}"
    for key in ("count", "total", "components"):
        if key in expect:
            assert same(result[key], expect[key]), f"{key}: {result[key]} != {expect[key]}"


class GraphApi(unittest.TestCase):
    def test_properties(self):
        graph = geoda.Graph(3, [(0, 1), [1, 2]], weights=[1, 2.5])
        self.assertEqual((graph.n, graph.directed, graph.weighted), (3, False, True))
        self.assertFalse(geoda.Graph(2, [], directed=True).weighted)

    def test_constructor_errors_are_value_errors(self):
        for n, edges, weights in [
            (2, [(0, 2)], None),
            (2, [(0, -1)], None),
            (2, [(0, 1)], [1.0, 2.0]),
        ]:
            with self.subTest(edges=edges, weights=weights), self.assertRaises(ValueError):
                geoda.Graph(n, edges, weights=weights)

    def test_queries_that_need_weights_are_type_errors(self):
        graph = geoda.Graph(2, [(0, 1)])
        with self.assertRaises(TypeError):
            graph.dijkstra(0)
        with self.assertRaises(TypeError):
            graph.mst()

    def test_negative_source_is_a_value_error(self):
        graph = geoda.Graph(2, [(0, 1)], weights=[1.0])
        for query in (graph.bfs, graph.dijkstra):
            with self.assertRaises(ValueError):
                query(-1)
            with self.assertRaises(ValueError):
                query(2)

    def test_mst_of_a_directed_graph_is_a_value_error(self):
        with self.assertRaises(ValueError):
            geoda.Graph(2, [(0, 1)], directed=True, weights=[1.0]).mst()

    def test_results_are_plain_python_values(self):
        paths = geoda.Graph(3, [(0, 1)], directed=True, weights=[2.0]).dijkstra(0)
        self.assertEqual(paths["distance"], [0.0, 2.0, math.inf])
        self.assertEqual(paths["parent"], [None, 0, None])
        self.assertEqual(geoda.Graph(2, [(1, 0)], directed=True).topological_order(), [1, 0])


if __name__ == "__main__":
    unittest.main()
