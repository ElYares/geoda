"""Pruebas del clustering de la fase 6: `kmeans`, `hierarchical`, `cut_tree`, y los casos de
`spec/cases/cluster/` que se ejecutan por el binding nativo (sin pasar por la capi)."""

import json
import math
import unittest
from pathlib import Path

import geoda

try:
    import pyarrow as pa
except ImportError:
    pa = None

CASES = sorted((Path(__file__).resolve().parents[3] / "spec" / "cases" / "cluster").glob("*.json"))
EXCEPTIONS = {
    "INVALID_ARGUMENT": ValueError,
    "LENGTH_MISMATCH": ValueError,
    "TYPE_MISMATCH": (TypeError, ValueError),
}
# Una lista de `str` no es una lista de numeros: en Python es `TypeError` antes de llegar al nucleo
# (la capi la rechaza con INVALID_ARGUMENT).
CASE_EXCEPTIONS = {"cluster/error_kmeans_text": TypeError}
X, Y = [0, 0, 10, 10], [0, 1, 10, 11]


def _values(column):
    if column["dtype"] == "float64":
        return [float(v) if isinstance(v, str) else v for v in column["values"]]
    return column["values"]


class KMeans(unittest.TestCase):
    def test_two_clouds(self):
        result = geoda.kmeans([X, Y], 2, seed=1)
        self.assertEqual(result["labels"], [0, 0, 1, 1])
        self.assertEqual(result["centroids"], [[0.0, 0.5], [10.0, 10.5]])
        self.assertEqual(result["inertia"], 1.0)
        self.assertGreaterEqual(result["iterations"], 1)

    def test_same_seed_same_result(self):
        first = geoda.kmeans([X, Y], 2, seed=7)
        for _ in range(3):
            self.assertEqual(geoda.kmeans([X, Y], 2, seed=7), first)

    def test_k_one_is_the_mean(self):
        result = geoda.kmeans([[1.0, 2.0, 3.0]], 1)
        self.assertEqual(result["centroids"], [[2.0]])
        self.assertEqual(result["inertia"], 2.0)

    def test_invalid_arguments(self):
        for call in (
            lambda: geoda.kmeans([X], 0),
            lambda: geoda.kmeans([X], 5),
            lambda: geoda.kmeans([[1.0, None, 3.0]], 1),
            lambda: geoda.kmeans([[1.0, float("nan")]], 1),
            lambda: geoda.kmeans([X, [1]], 1),
            lambda: geoda.kmeans([X], 2, max_iter=0),
        ):
            with self.assertRaises(ValueError):
                call()

    @unittest.skipIf(pa is None, "pyarrow no esta instalado")
    def test_arrow_columns(self):
        result = geoda.kmeans([pa.array(X, pa.int32()), pa.array(Y, pa.int64())], 2, seed=1)
        self.assertEqual(result["labels"], [0, 0, 1, 1])


class Hierarchical(unittest.TestCase):
    def test_dendrogram_and_cut(self):
        d = geoda.hierarchical([[0.0, 1.0, 5.0, 6.0]])
        self.assertEqual(d["n"], 4)
        self.assertEqual(d["left"], [0, 2, 4])
        self.assertEqual(d["right"], [1, 3, 5])
        self.assertEqual(d["height"], [1.0, 1.0, 4.0])
        self.assertEqual(d["size"], [2, 2, 4])
        self.assertEqual(geoda.cut_tree(d, 2), [0, 0, 1, 1])
        self.assertEqual(geoda.cut_tree(d, 4), [0, 1, 2, 3])

    def test_heights_do_not_decrease(self):
        d = geoda.hierarchical([[3.0, 1.0, 4.0, 1.5, 9.0, 2.6], [5.0, 3.0, 5.0, 8.0, 9.0, 7.0]])
        self.assertEqual(len(d["height"]), 5)
        self.assertEqual(d["height"], sorted(d["height"]))

    def test_one_row(self):
        d = geoda.hierarchical([[3.0]])
        self.assertEqual((d["left"], d["height"]), ([], []))
        self.assertEqual(geoda.cut_tree(d, 1), [0])

    def test_errors(self):
        d = geoda.hierarchical([[0.0, 1.0, 5.0]])
        with self.assertRaises(ValueError):
            geoda.cut_tree(d, 0)
        with self.assertRaises(ValueError):
            geoda.cut_tree(d, 4)
        with self.assertRaises(KeyError):
            geoda.cut_tree({"n": 3, "left": [0, 2]}, 2)
        with self.assertRaises(ValueError):
            geoda.cut_tree({"n": 3, "left": [0, 9], "right": [1, 3]}, 2)
        with self.assertRaises(ValueError):
            geoda.hierarchical([[0.0] * 5001])


class SpecCases(unittest.TestCase):
    """Los casos `kmeans`, `hierarchical` y `dendrogram_cut` dan lo mismo por el binding."""

    def test_cases(self):
        ran = 0
        for path in CASES:
            case = json.loads(path.read_text(encoding="utf-8"))
            op, expect = case["op"], case["expect"]
            with self.subTest(case=case["id"]):
                ran += 1
                args = case.get("args", {})
                if op == "kmeans":
                    kwargs = {k: v for k, v in args.items() if k in ("seed", "max_iter", "tol")}
                    call = lambda: geoda.kmeans(  # noqa: E731
                        [_values(c) for c in case["inputs"]], args["k"], **kwargs
                    )
                elif op == "hierarchical":
                    call = lambda: geoda.hierarchical([_values(c) for c in case["inputs"]])  # noqa: E731
                else:
                    dendrogram = {
                        "n": args["n"],
                        "left": case["input"]["values"],
                        "right": case["input2"]["values"],
                    }
                    call = lambda: geoda.cut_tree(dendrogram, args["k"])  # noqa: E731
                if "error" in expect:
                    with self.assertRaises(
                        CASE_EXCEPTIONS.get(case["id"], EXCEPTIONS[expect["error"]])
                    ):
                        call()
                    continue
                result = call()
                columns = [c["values"] for c in expect["columns"]]
                if op == "kmeans":
                    flat = [v for row in result["centroids"] for v in row]
                    self.assertEqual([result["labels"], flat], columns)
                    self.assertEqual(result["inertia"], expect["inertia"])
                    self.assertEqual(result["iterations"], expect["iterations"])
                elif op == "hierarchical":
                    self.assertEqual(
                        [result["left"], result["right"], result["height"], result["size"]], columns
                    )
                else:
                    self.assertEqual([result], columns)
        self.assertGreaterEqual(ran, 20)
        self.assertTrue(math.isfinite(ran))


if __name__ == "__main__":
    unittest.main()
