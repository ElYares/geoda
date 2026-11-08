"""Pruebas de `geoda.LinearRegression` y `geoda.PCA` (fase 7) y de los casos de `spec/cases/model/`
por el binding nativo (sin pasar por la capi)."""

import json
import unittest
from pathlib import Path

import geoda

try:
    import pyarrow as pa
except ImportError:
    pa = None

CASES = sorted((Path(__file__).resolve().parents[3] / "spec" / "cases" / "model").glob("*.json"))


def _values(column):
    if column["dtype"] == "float64":
        return [float(v) if isinstance(v, str) else v for v in column["values"]]
    return column["values"]


def _flat(rows):
    return [x for row in rows for x in row]


class LinearRegression(unittest.TestCase):
    X = [0.0, 1.0, 2.0, 3.0]
    Y = [1.0, 3.0, 5.0, 7.0]

    def test_exact_line(self):
        model = geoda.LinearRegression()
        self.assertIs(model.fit([self.X], self.Y), model)
        self.assertAlmostEqual(model.coef_[0], 2.0, places=12)
        self.assertAlmostEqual(model.intercept_, 1.0, places=12)
        self.assertAlmostEqual(model.r2_, 1.0, places=12)
        self.assertAlmostEqual(model.predict([[10.0]])[0], 21.0, places=10)

    def test_without_intercept(self):
        model = geoda.LinearRegression(fit_intercept=False).fit([[1.0, 2.0, 3.0]], [2.0, 4.0, 6.0])
        self.assertEqual(model.intercept_, 0.0)
        self.assertAlmostEqual(model.coef_[0], 2.0, places=12)

    def test_two_variables(self):
        x1, x2 = [1.0, 2.0, 3.0, 4.0, 5.0], [2.0, 1.0, 4.0, 3.0, 7.0]
        y = [1 + 2 * a - 3 * b for a, b in zip(x1, x2)]
        model = geoda.LinearRegression().fit([x1, x2], y)
        self.assertAlmostEqual(model.coef_[0], 2.0, places=9)
        self.assertAlmostEqual(model.coef_[1], -3.0, places=9)
        self.assertAlmostEqual(model.intercept_, 1.0, places=9)

    def test_not_fitted(self):
        model = geoda.LinearRegression()
        with self.assertRaises(RuntimeError):
            model.predict([self.X])
        for name in ("coef_", "intercept_", "r2_"):
            with self.assertRaises(AttributeError):
                getattr(model, name)

    def test_errors(self):
        for call in (
            lambda: geoda.LinearRegression().fit([[1.0, 2.0, 3.0, 4.0], [2.0, 4.0, 6.0, 8.0]], [1, 2, 3, 5]),
            lambda: geoda.LinearRegression().fit([[1.0, 2.0]], [1.0, 2.0, 3.0]),
            lambda: geoda.LinearRegression().fit([[1.0, 2.0], [3.0, 5.0], [1.0, 0.0]], [1.0, 2.0]),
            lambda: geoda.LinearRegression().fit([[1.0, 2.0, 3.0]], [1.0, float("nan"), 3.0]),
        ):
            with self.assertRaises(ValueError):
                call()
        model = geoda.LinearRegression().fit([self.X], self.Y)
        with self.assertRaises(ValueError):
            model.predict([self.X, self.X])

    @unittest.skipIf(pa is None, "pyarrow no esta instalado")
    def test_arrow(self):
        model = geoda.LinearRegression().fit([pa.array(self.X)], pa.array(self.Y))
        self.assertAlmostEqual(model.coef_[0], 2.0, places=12)


class PCA(unittest.TestCase):
    X = [1.0, 2.0, 3.0, 4.0, 5.0]
    Y = [2.0, 4.0, 6.0, 8.0, 10.0]

    def test_correlated_variables(self):
        pca = geoda.PCA(2).fit([self.X, self.Y])
        self.assertAlmostEqual(pca.explained_variance_ratio_[0], 1.0, places=12)
        self.assertAlmostEqual(pca.explained_variance_ratio_[1], 0.0, places=12)
        self.assertAlmostEqual(pca.explained_variance_[0], 12.5, places=12)
        self.assertEqual(pca.mean_, [3.0, 6.0])
        self.assertAlmostEqual(pca.components_[0][0], 1 / 5**0.5, places=12)
        self.assertAlmostEqual(pca.components_[0][1], 2 / 5**0.5, places=12)

    def test_transform(self):
        pca = geoda.PCA(1).fit([self.X, self.Y])
        projected = pca.transform([self.X, self.Y])
        self.assertEqual(len(projected), 5)
        for row, x in zip(projected, self.X):
            self.assertAlmostEqual(row[0], 5**0.5 * (x - 3.0), places=9)

    def test_projection_has_zero_mean_and_the_explained_variance(self):
        data = [[2.5, 0.5, 2.2, 1.9, 3.1, 2.3, 2.0, 1.0, 1.5, 1.1], [2.4, 0.7, 2.9, 2.2, 3.0, 2.7, 1.6, 1.1, 1.6, 0.9]]
        pca = geoda.PCA(2).fit(data)
        projected = pca.transform(data)
        for k in range(2):
            column = [row[k] for row in projected]
            mean = sum(column) / 10
            variance = sum((x - mean) ** 2 for x in column) / 9
            self.assertAlmostEqual(mean, 0.0, places=9)
            self.assertAlmostEqual(variance, pca.explained_variance_[k], places=9)

    def test_deterministic(self):
        data = [[1.0, 4.0, 2.0, 8.0], [3.0, 1.0, 5.0, 2.0]]
        first = geoda.PCA(2).fit(data)
        for _ in range(3):
            again = geoda.PCA(2).fit(data)
            self.assertEqual(again.components_, first.components_)

    def test_not_fitted(self):
        pca = geoda.PCA(1)
        with self.assertRaises(RuntimeError):
            pca.transform([self.X])
        with self.assertRaises(AttributeError):
            pca.components_

    def test_errors(self):
        for call in (
            lambda: geoda.PCA(0).fit([self.X]),
            lambda: geoda.PCA(2).fit([self.X]),
            lambda: geoda.PCA(1).fit([[1.0], [2.0]]),
            lambda: geoda.PCA(1).fit([[1.0, float("nan"), 3.0]]),
        ):
            with self.assertRaises(ValueError):
                call()
        pca = geoda.PCA(1).fit([self.X, self.Y])
        with self.assertRaises(ValueError):
            pca.transform([self.X])


class SpecCases(unittest.TestCase):
    """Los casos `regression_fit_predict` y `pca_fit_transform` de la spec dan lo mismo por el binding."""

    def test_cases(self):
        ran = 0
        for path in CASES:
            case = json.loads(path.read_text(encoding="utf-8"))
            op, expect, args = case["op"], case["expect"], case.get("args", {})
            with self.subTest(case=case["id"]):
                ran += 1
                if not case["inputs"] or any(None in c["values"] for c in case["inputs"] + [case.get("y", {"values": []})]):
                    if "error" in expect:
                        continue
                train = [_values(c) for c in case["inputs"]]
                rows = [_values(c) for c in case.get("predict_inputs", case["inputs"])]
                if op == "regression_fit_predict":
                    def call():
                        model = geoda.LinearRegression(args.get("fit_intercept", True)).fit(
                            train, _values(case["y"])
                        )
                        return model, model.predict(rows)
                else:
                    def call():
                        model = geoda.PCA(args["n_components"]).fit(train)
                        return model, model.transform(rows)
                if "error" in expect:
                    with self.assertRaises(ValueError):
                        call()
                    continue
                model, output = call()
                tolerance = expect["tolerance"]
                if op == "regression_fit_predict":
                    got = [model.coef_, output]
                    self.assertLessEqual(abs(model.intercept_ - expect["intercept"]), tolerance)
                    self.assertLessEqual(abs(model.r2_ - expect["r_squared"]), tolerance)
                else:
                    got = [
                        _flat(model.components_),
                        model.explained_variance_,
                        model.explained_variance_ratio_,
                        model.mean_,
                        _flat(output),
                    ]
                wanted = [c["values"] for c in expect["columns"]]
                self.assertEqual(len(got), len(wanted))
                for g, w in zip(got, wanted):
                    self.assertEqual(len(g), len(w))
                    for a, b in zip(g, w):
                        self.assertLessEqual(abs(a - b), tolerance)
        self.assertGreaterEqual(ran, 17)


if __name__ == "__main__":
    unittest.main()
