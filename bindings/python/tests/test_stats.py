"""Estadisticos escalares de `geoda` desde Python (HU-026): listas y columnas Arrow."""

import math
import unittest

import geoda

try:
    import pyarrow as pa
except ImportError:  # pragma: no cover - depende del entorno
    pa = None


class ListInput(unittest.TestCase):
    def test_basic_statistics(self):
        values = [1, 2, None, 4]
        self.assertEqual(geoda.count(values), 3)
        self.assertEqual(geoda.sum(values), 7.0)
        self.assertEqual(geoda.min(values), 1.0)
        self.assertEqual(geoda.max(values), 4.0)
        self.assertEqual(geoda.mean(values), 7 / 3)

    def test_lists_are_float_columns_so_results_are_float(self):
        self.assertIsInstance(geoda.sum([1, 2, 3]), float)

    def test_var_and_std_default_to_ddof_1(self):
        values = [1, 2, 3, 4]
        self.assertEqual(geoda.var(values), 5 / 3)
        self.assertEqual(geoda.var(values, ddof=1), 5 / 3)
        self.assertEqual(geoda.var(values, ddof=0), 1.25)
        self.assertEqual(geoda.std(values, 0), math.sqrt(1.25))
        self.assertEqual(geoda.std(values), math.sqrt(5 / 3))

    def test_corr(self):
        self.assertEqual(geoda.corr([1, 2, 3, 4], [2, 4, 6, 8]), 1.0)
        self.assertEqual(geoda.corr([1, None, 3, 4], [8, 100, 4, 2]), -1.0)

    def test_nan_propagates(self):
        values = [1.0, float("nan"), 3.0]
        for fn in (geoda.sum, geoda.min, geoda.max, geoda.mean):
            with self.subTest(fn=fn.__name__):
                self.assertTrue(math.isnan(fn(values)))
        self.assertEqual(geoda.count(values), 3)

    def test_errors_are_value_errors(self):
        for call in (
            lambda: geoda.mean([]),
            lambda: geoda.min([None]),
            lambda: geoda.max([]),
            lambda: geoda.var([1, 2], ddof=2),
            lambda: geoda.std([1], ddof=1),
            lambda: geoda.corr([1, 2, 3], [1, 2]),
            lambda: geoda.corr([5, 5, 5], [1, 2, 3]),
        ):
            with self.subTest(call=call):
                with self.assertRaises(ValueError):
                    call()

    def test_empty_sum_and_count_are_zero(self):
        self.assertEqual(geoda.sum([None, None]), 0.0)
        self.assertEqual(geoda.count([]), 0)

    def test_negative_ddof_is_rejected(self):
        with self.assertRaises(OverflowError):
            geoda.var([1, 2, 3], ddof=-1)


@unittest.skipIf(pa is None, "pyarrow no esta instalado")
class ArrowInput(unittest.TestCase):
    def test_spec_examples(self):
        self.assertEqual(geoda.mean(pa.array([1, 2, None, 4])), 7 / 3)
        x, y = pa.array([1, 2, 3, 4]), pa.array([2.0, 4.0, 6.0, 8.0])
        self.assertEqual(geoda.corr(x, y), 1.0)
        self.assertEqual(geoda.var(pa.array([1, 2, 3, 4])), 5 / 3)

    def test_integer_columns_give_ints_and_float_columns_give_floats(self):
        ints = pa.array([5, None, -3, 9], pa.int64())
        for fn, want in ((geoda.sum, 11), (geoda.min, -3), (geoda.max, 9)):
            with self.subTest(fn=fn.__name__):
                got = fn(ints)
                self.assertEqual(got, want)
                self.assertIs(type(got), int)
        floats = pa.array([5.5, None, -3.0], pa.float64())
        self.assertIs(type(geoda.sum(floats)), float)
        self.assertEqual(geoda.min(floats), -3.0)
        self.assertEqual(geoda.count(pa.array([1, None, 3], pa.int32())), 2)

    def test_int_sum_that_does_not_fit_is_value_error(self):
        with self.assertRaises(ValueError):
            geoda.sum(pa.array([2**63 - 1, 1], pa.int64()))

    def test_sliced_columns_use_the_window(self):
        arr = pa.array([100, 1, 2, 3, 900], pa.int64()).slice(1, 3)
        self.assertEqual(geoda.sum(arr), 6)
        self.assertEqual(geoda.count(arr), 3)

    def test_bool_columns_count_as_zero_and_one(self):
        arr = pa.array([True, None, False, True])
        self.assertEqual(geoda.sum(arr), 2.0)
        self.assertEqual(geoda.mean(arr), 2 / 3)

    def test_unsupported_types_are_type_errors(self):
        for name, arr in {
            "texto": pa.array(["a", "b"]),
            "decimal128": pa.array([1, 2], pa.decimal128(10, 2)),
        }.items():
            with self.subTest(name):
                for fn in (geoda.count, geoda.sum, geoda.mean, geoda.min):
                    with self.assertRaises(TypeError):
                        fn(arr)

    def test_corr_with_different_lengths_is_value_error(self):
        with self.assertRaises(ValueError):
            geoda.corr(pa.array([1, 2, 3]), pa.array([1, 2]))


class Workers(unittest.TestCase):
    """HU-030: `workers` reparte los bloques entre hilos sin cambiar ni un bit del resultado."""

    # Varios bloques de 1024 y magnitudes muy distintas: el orden de la suma cambia los bits.
    DATA = [math.sin(i * 0.1) * 10.0 ** (i % 13 - 6) for i in range(10_000)]
    OTHER = [math.cos(i * 0.1) + i * 1e-4 for i in range(10_000)]

    def assertSameBits(self, got, want):
        if isinstance(want, float):
            self.assertEqual(float.hex(got), float.hex(want))
        else:
            self.assertEqual(got, want)

    def test_scalar_results_do_not_depend_on_workers(self):
        calls = {
            "sum": lambda w: geoda.sum(self.DATA, workers=w),
            "min": lambda w: geoda.min(self.DATA, workers=w),
            "max": lambda w: geoda.max(self.DATA, workers=w),
            "mean": lambda w: geoda.mean(self.DATA, workers=w),
            "var": lambda w: geoda.var(self.DATA, workers=w),
            "std": lambda w: geoda.std(self.DATA, ddof=0, workers=w),
            "corr": lambda w: geoda.corr(self.DATA, self.OTHER, workers=w),
        }
        for name, call in calls.items():
            reference = call(1)
            for workers in (2, 4, 8):
                with self.subTest(name=name, workers=workers):
                    self.assertSameBits(call(workers), reference)

    def test_histogram_and_group_by_do_not_depend_on_workers(self):
        reference = geoda.histogram(self.DATA, 10, range=(-1.0, 1.0))
        keys = [i % 3 for i in range(10_000)]
        grouped = geoda.group_by(keys).mean(self.DATA)
        for workers in (2, 4, 8):
            self.assertEqual(geoda.histogram(self.DATA, 10, range=(-1.0, 1.0), workers=workers), reference)
            got = geoda.group_by(keys).mean(self.DATA, workers=workers)
            self.assertEqual(got["keys"], grouped["keys"])
            for a, b in zip(got["values"], grouped["values"]):
                self.assertSameBits(a, b)

    def test_default_is_one_worker(self):
        self.assertSameBits(geoda.mean(self.DATA), geoda.mean(self.DATA, workers=1))

    def test_zero_or_negative_workers_is_value_error(self):
        for workers in (0, -1):
            for call in (
                lambda: geoda.sum([1.0, 2.0], workers=workers),
                lambda: geoda.mean([1.0, 2.0], workers=workers),
                lambda: geoda.var([1.0, 2.0], workers=workers),
                lambda: geoda.corr([1.0, 2.0], [1.0, 3.0], workers=workers),
                lambda: geoda.histogram([1.0, 2.0], 2, workers=workers),
                lambda: geoda.group_by([1, 2]).sum([1.0, 2.0], workers=workers),
            ):
                with self.subTest(workers=workers), self.assertRaises(ValueError):
                    call()


if __name__ == "__main__":
    unittest.main()
