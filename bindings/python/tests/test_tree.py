"""Pruebas de `geoda.DecisionTree` (fase 6) y de los casos de `spec/cases/tree/` por el binding."""

import json
import unittest
from pathlib import Path

import geoda

try:
    import pyarrow as pa
except ImportError:
    pa = None

CASES = sorted((Path(__file__).resolve().parents[3] / "spec" / "cases" / "tree").glob("*.json"))
EXCEPTIONS = {
    "INVALID_ARGUMENT": ValueError,
    "LENGTH_MISMATCH": ValueError,
    "TYPE_MISMATCH": (TypeError, ValueError),
}
# El binding recibe las etiquetas como enteros: una columna `float64` o un `None` no llegan al
# nucleo (la capi los rechaza con TYPE_MISMATCH / INVALID_ARGUMENT).
CASE_EXCEPTIONS = {
    "tree/error_labels_type": (TypeError, ValueError),
    "tree/error_null_label": TypeError,
}
X = [1.0, 2.0, 3.0, 10.0, 11.0, 12.0]
Y = [0, 0, 0, 1, 1, 1]


def _values(column):
    if column["dtype"] == "float64":
        return [float(v) if isinstance(v, str) else v for v in column["values"]]
    return column["values"]


class DecisionTreeClass(unittest.TestCase):
    def test_fit_returns_the_tree_and_predicts(self):
        tree = geoda.DecisionTree()
        self.assertIs(tree.fit([X], Y), tree)
        self.assertEqual(tree.predict([X]), Y)
        self.assertEqual((tree.n_nodes, tree.depth), (3, 1))

    def test_predict_new_rows(self):
        tree = geoda.DecisionTree().fit([X], Y)
        self.assertEqual(tree.predict([[0.0, 6.6, 6.4, 100.0]]), [0, 1, 0, 1])

    def test_parameters(self):
        self.assertEqual(geoda.DecisionTree(max_depth=0).fit([X], Y).n_nodes, 1)
        self.assertEqual(geoda.DecisionTree(min_samples_leaf=4).fit([X], Y).n_nodes, 1)

    def test_two_features(self):
        tree = geoda.DecisionTree().fit([[1.0] * 4, [1, 2, 8, 9]], [0, 0, 1, 1])
        self.assertEqual(tree.predict([[1.0] * 4, [1, 2, 8, 9]]), [0, 0, 1, 1])

    def test_it_is_deterministic(self):
        data = [[1.0, 2, 3, 4, 5, 6], [6.0, 5, 4, 3, 2, 1]]
        labels = [0, 0, 1, 1, 0, 1]
        reference = geoda.DecisionTree().fit(data, labels).predict(data)
        for _ in range(3):
            self.assertEqual(geoda.DecisionTree().fit(data, labels).predict(data), reference)

    def test_predict_before_fit(self):
        tree = geoda.DecisionTree()
        self.assertEqual((tree.n_nodes, tree.depth), (0, 0))
        with self.assertRaises(RuntimeError):
            tree.predict([X])

    def test_errors(self):
        for call in (
            lambda: geoda.DecisionTree().fit([X], [0, 1]),
            lambda: geoda.DecisionTree().fit([X], [0, 0, 0, 1, 1, -1]),
            lambda: geoda.DecisionTree(min_samples_leaf=0).fit([X], Y),
            lambda: geoda.DecisionTree().fit([[1.0, float("nan")]], [0, 1]),
            lambda: geoda.DecisionTree().fit([[]], []),
        ):
            with self.assertRaises(ValueError):
                call()
        tree = geoda.DecisionTree().fit([X], Y)
        with self.assertRaises(ValueError):
            tree.predict([X, X])

    @unittest.skipIf(pa is None, "pyarrow no esta instalado")
    def test_arrow_inputs(self):
        tree = geoda.DecisionTree().fit([pa.array(X)], pa.array(Y, pa.int64()))
        self.assertEqual(tree.predict([pa.array(X)]), Y)
        with self.assertRaises(TypeError):
            geoda.DecisionTree().fit([pa.array(X)], pa.array([0.0, 1.0] * 3))


class SpecCases(unittest.TestCase):
    """Los casos `tree_fit_predict` de la spec dan lo mismo por el binding."""

    def test_cases(self):
        ran = 0
        for path in CASES:
            case = json.loads(path.read_text(encoding="utf-8"))
            expect, args = case["expect"], case.get("args", {})
            with self.subTest(case=case["id"]):
                ran += 1

                def call():
                    tree = geoda.DecisionTree(
                        args.get("max_depth", 8), args.get("min_samples_leaf", 1)
                    ).fit([_values(c) for c in case["inputs"]], _values(case["labels"]))
                    rows = case.get("predict_inputs", case["inputs"])
                    return tree, tree.predict([_values(c) for c in rows])

                if "error" in expect:
                    with self.assertRaises(
                        CASE_EXCEPTIONS.get(case["id"], EXCEPTIONS[expect["error"]])
                    ):
                        call()
                    continue
                tree, predicted = call()
                self.assertEqual([predicted], [c["values"] for c in expect["columns"]])
                self.assertEqual(tree.n_nodes, expect["nodes"])
        self.assertGreaterEqual(ran, 14)


if __name__ == "__main__":
    unittest.main()
