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


class Factorizations(unittest.TestCase):
    def test_qr_reconstructs(self):
        a = [[1.0, 2.0], [3.0, 4.0], [5.0, 6.0]]
        f = geoda.qr(a)
        product = geoda.matmul(f["q"], f["r"])
        for row, source in zip(product, a):
            for g, w in zip(row, source):
                self.assertAlmostEqual(g, w, places=12)
        self.assertEqual(f["r"][1][0], 0.0)

    def test_qr_wide_is_an_error(self):
        with self.assertRaises(ValueError):
            geoda.qr([[1, 2, 3], [4, 5, 6]])

    def test_svd_reconstructs_and_orders(self):
        a = [[2.0, -1.0, 0.5], [-3.0, 4.0, 1.0], [1.0, 1.0, -2.0], [0.0, 2.0, 2.0]]
        f = geoda.svd(a)
        self.assertEqual(f["s"], sorted(f["s"], reverse=True))
        scaled = [[u * s for u, s in zip(row, f["s"])] for row in f["u"]]
        product = geoda.matmul(scaled, f["vt"])
        for row, source in zip(product, a):
            for g, w in zip(row, source):
                self.assertAlmostEqual(g, w, places=9)

    def test_svd_is_deterministic(self):
        a = [[1, 2], [3, 4], [5, 7]]
        self.assertEqual(geoda.svd(a), geoda.svd(a))

    def test_svd_rank_one(self):
        f = geoda.svd([[1, 2], [2, 4]])
        self.assertAlmostEqual(f["s"][0], 5.0, places=12)
        self.assertAlmostEqual(f["s"][1], 0.0, places=12)

    def test_lstsq(self):
        x = geoda.lstsq([[1, 0], [1, 1], [1, 2]], [1, 3, 5])
        self.assertAlmostEqual(x[0], 1.0, places=12)
        self.assertAlmostEqual(x[1], 2.0, places=12)

    def test_lstsq_errors(self):
        for call in (
            lambda: geoda.lstsq([[1, 2], [2, 4], [3, 6]], [1, 2, 3]),
            lambda: geoda.lstsq([[1, 2, 3]], [1]),
            lambda: geoda.lstsq([[1, 0], [1, 1]], [1]),
        ):
            with self.assertRaises(ValueError):
                call()


class SpecCases(unittest.TestCase):
    """Los casos de `spec/cases/linalg/` dan lo mismo por el binding."""

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
                    call = lambda: [flat(geoda.matmul(a, rows_of(case["inputs2"])))]  # noqa: E731
                elif op == "lu_solve":
                    call = lambda: [geoda.solve(a, case["input"]["values"])]  # noqa: E731
                elif op == "det":
                    call = lambda: [[geoda.det(a)]]  # noqa: E731
                elif op == "inverse":
                    call = lambda: [flat(geoda.inv(a))]  # noqa: E731
                elif op == "qr":
                    call = lambda: [  # noqa: E731
                        flat(geoda.qr(a)["q"]),
                        flat(geoda.qr(a)["r"]),
                    ]
                elif op == "svd":
                    call = lambda: [  # noqa: E731
                        flat(geoda.svd(a)["u"]),
                        geoda.svd(a)["s"],
                        flat(geoda.svd(a)["vt"]),
                    ]
                else:
                    call = lambda: [geoda.lstsq(a, case["input"]["values"])]  # noqa: E731
                if "error" in expect:
                    with self.assertRaises(ValueError):
                        call()
                    continue
                result = call()
                tolerance = expect.get("tolerance")
                wanted = [[expect["value"]]] if op == "det" else [c["values"] for c in expect["columns"]]
                self.assertEqual(len(result), len(wanted))
                for got, want in zip(result, wanted):
                    self.assertEqual(len(got), len(want))
                    for g, w in zip(got, want):
                        if tolerance is None:
                            self.assertEqual(g, w)
                        else:
                            self.assertLessEqual(abs(g - w), tolerance)
        self.assertGreaterEqual(ran, 29)


if __name__ == "__main__":
    unittest.main()
