"""Pruebas del algebra lineal basica (fase 7): `matmul`, `solve`, `det`, `inv`, y los casos de
`spec/cases/linalg/` por el binding nativo.

En Python una matriz es una **lista de filas**; la spec las da como lista de columnas, asi que los
casos se transponen aqui (decision 014, N9).
"""

import json
import unittest
from pathlib import Path

import geoda

CASES = sorted((Path(__file__).resolve().parents[3] / "spec" / "cases" / "linalg").glob("*.json"))


def rows_of(columns):
    """Una matriz dada como lista de columnas de la spec, como lista de filas."""
    values = [c["values"] for c in columns]
    if not values:
        return []
    return [list(row) for row in zip(*values)]


def flat(matrix):
    return [x for row in matrix for x in row]


class Matmul(unittest.TestCase):
    def test_product(self):
        a = [[1, 2, 3], [4, 5, 6]]
        b = [[7, 8], [9, 10], [11, 12]]
        self.assertEqual(geoda.matmul(a, b), [[58.0, 64.0], [139.0, 154.0]])

    def test_identity(self):
        a = [[1.5, -2.0], [0.25, 4.0]]
        self.assertEqual(geoda.matmul(a, [[1, 0], [0, 1]]), a)

    def test_errors(self):
        a = [[1, 2, 3], [4, 5, 6]]
        for call in (
            lambda: geoda.matmul(a, a),
            lambda: geoda.matmul([[1, 2], [3]], [[1], [2]]),
            lambda: geoda.matmul([[1, float("nan")]], [[1], [2]]),
            lambda: geoda.matmul([[float("inf")]], [[1]]),
        ):
            with self.assertRaises(ValueError):
                call()


class Solve(unittest.TestCase):
    def test_2x2(self):
        x = geoda.solve([[2, 1], [1, 3]], [3, 5])
        self.assertAlmostEqual(x[0], 0.8, places=12)
        self.assertAlmostEqual(x[1], 1.4, places=12)

    def test_pivoting(self):
        self.assertEqual(geoda.solve([[0, 1], [1, 0]], [2, 3]), [3.0, 2.0])

    def test_errors(self):
        for call in (
            lambda: geoda.solve([[1, 2], [2, 4]], [1, 2]),
            lambda: geoda.solve([[1, 2, 3], [4, 5, 6]], [1, 2]),
            lambda: geoda.solve([[1, 0], [0, 1]], [1]),
        ):
            with self.assertRaises(ValueError):
                call()


class DetAndInverse(unittest.TestCase):
    def test_det(self):
        self.assertAlmostEqual(geoda.det([[4, 3], [6, 3]]), -6.0, places=12)
        self.assertEqual(geoda.det([[1, 0], [0, 1]]), 1.0)
        self.assertEqual(geoda.det([[1, 2], [2, 4]]), 0.0)
        with self.assertRaises(ValueError):
            geoda.det([[1, 2, 3], [4, 5, 6]])

    def test_inverse(self):
        a = [[4, 7, 2], [3, 6, 1], [2, 5, 3]]
        product = geoda.matmul(a, geoda.inv(a))
        for i, row in enumerate(product):
            for j, value in enumerate(row):
                self.assertAlmostEqual(value, 1.0 if i == j else 0.0, places=9)

    def test_inverse_errors(self):
        with self.assertRaises(ValueError):
            geoda.inv([[1, 2], [2, 4]])
        with self.assertRaises(ValueError):
            geoda.inv([[1, 2, 3]])


class SpecCases(unittest.TestCase):
    """Los casos `matmul`, `lu_solve`, `det` e `inverse` de la spec dan lo mismo por el binding."""

    def test_cases(self):
        ran = 0
        for path in CASES:
            case = json.loads(path.read_text(encoding="utf-8"))
            op, expect = case["op"], case["expect"]
            with self.subTest(case=case["id"]):
                ran += 1
                inputs = case["inputs"]
                # Un nulo o una lista vacia no se pueden armar como filas de Python; son errores de
                # la frontera de la capi.
                if any(None in c["values"] for c in inputs) or not inputs:
                    if "error" in expect:
                        continue
                a = rows_of(inputs)
                if op == "matmul":
                    call = lambda: flat(geoda.matmul(a, rows_of(case["inputs2"])))  # noqa: E731
                elif op == "lu_solve":
                    call = lambda: geoda.solve(a, case["input"]["values"])  # noqa: E731
                elif op == "det":
                    call = lambda: geoda.det(a)  # noqa: E731
                else:
                    call = lambda: flat(geoda.inv(a))  # noqa: E731
                if "error" in expect:
                    with self.assertRaises(ValueError):
                        call()
                    continue
                result = call()
                tolerance = expect.get("tolerance")
                want = expect["value"] if op == "det" else expect["columns"][0]["values"]
                got = [result] if op == "det" else result
                want = [want] if op == "det" else want
                self.assertEqual(len(got), len(want))
                for g, w in zip(got, want):
                    if tolerance is None:
                        self.assertEqual(g, w)
                    else:
                        self.assertLessEqual(abs(g - w), tolerance)
        self.assertGreaterEqual(ran, 16)


if __name__ == "__main__":
    unittest.main()
