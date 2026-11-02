"""`geoda.histogram` y `geoda.group_by` desde Python (HU-027): listas y columnas Arrow."""

import math
import unittest

import geoda

try:
    import pyarrow as pa
except ImportError:  # pragma: no cover - depende del entorno
    pa = None


class Histogram(unittest.TestCase):
    def test_spec_example(self):
        h = geoda.histogram([1, 2, 2, 3, 4, 4, 4, 5], bins=4)
        self.assertEqual(h["edges"], [1.0, 2.0, 3.0, 4.0, 5.0])
        self.assertEqual(h["counts"], [1, 2, 1, 4])
        self.assertEqual((h["below"], h["above"], h["nulls"], h["nans"]), (0, 0, 0, 0))

    def test_explicit_range_counts_the_rest_apart(self):
        h = geoda.histogram([1, 2, 3, 4, 5, 6], 2, range=(2, 4))
        self.assertEqual(h["edges"], [2.0, 3.0, 4.0])
        self.assertEqual(h["counts"], [1, 2])
        self.assertEqual((h["below"], h["above"]), (1, 2))

    def test_nulls_and_nans_are_counted_apart(self):
        h = geoda.histogram([1.0, None, float("nan"), 3.0], 2)
        self.assertEqual(h["counts"], [1, 1])
        self.assertEqual((h["nulls"], h["nans"]), (1, 1))

    def test_invalid_arguments_are_value_errors(self):
        for call in (
            lambda: geoda.histogram([1, 2, 3], 0),
            lambda: geoda.histogram([1, 2, 3], 2, range=(5, 1)),
            lambda: geoda.histogram([], 2),
            lambda: geoda.histogram([None], 2),
        ):
            with self.subTest(call=call):
                with self.assertRaises(ValueError):
                    call()

    def test_range_must_have_two_numbers(self):
        with self.assertRaises(ValueError):
            geoda.histogram([1, 2, 3], 2, range=(1, 2, 3))


class GroupByLists(unittest.TestCase):
    def test_sum_by_group(self):
        got = geoda.group_by([1, 2, 1, 2, 1]).sum([1, 2, 3, 4, 5])
        self.assertEqual(got, {"keys": [1.0, 2.0], "values": [9.0, 6.0]})

    def test_all_aggregates(self):
        g = geoda.group_by([1, 1, 1, 2, 2])
        values = [1, 3, 5, 2, 4]
        self.assertEqual(g.count(values)["values"], [3, 2])
        self.assertEqual(g.mean(values)["values"], [3.0, 3.0])
        self.assertEqual(g.min(values)["values"], [1.0, 2.0])
        self.assertEqual(g.max(values)["values"], [5.0, 4.0])
        self.assertEqual(g.var(values)["values"], [4.0, 2.0])
        self.assertEqual(g.var(values, ddof=0)["values"], [8 / 3, 1.0])
        self.assertEqual(g.std(values, ddof=0)["values"], [math.sqrt(8 / 3), 1.0])

    def test_null_key_is_a_group_with_key_none(self):
        got = geoda.group_by([1, None, 1, None]).sum([1, 10, 2, 20])
        self.assertEqual(got["keys"], [1.0, None])
        self.assertEqual(got["values"], [3.0, 30.0])

    def test_nan_is_one_group_and_signed_zeros_are_two(self):
        got = geoda.group_by([float("nan"), -0.0, 0.0, float("nan")]).count([1, 2, 4, 8])
        self.assertEqual(got["values"], [2, 1, 1])
        self.assertTrue(math.isnan(got["keys"][0]))
        self.assertEqual([math.copysign(1.0, k) for k in got["keys"][1:]], [-1.0, 1.0])

    def test_errors_are_value_errors(self):
        g = geoda.group_by([1, 2, 3])
        for call in (
            lambda: g.sum([1, 2]),
            lambda: geoda.group_by([1, 2]).mean([1, None]),
            lambda: geoda.group_by([1, 1]).var([1, 2], ddof=2),
            lambda: geoda._geoda.group_aggregate([1], [1], "median"),
        ):
            with self.subTest(call=call):
                with self.assertRaises(ValueError):
                    call()


@unittest.skipIf(pa is None, "pyarrow no esta instalado")
class Arrow(unittest.TestCase):
    def test_histogram_of_a_pyarrow_column(self):
        h = geoda.histogram(pa.array([1, 2, 2, 3, 4, 4, 4, 5]), bins=4)
        self.assertEqual(h["counts"], [1, 2, 1, 4])

    def test_group_by_with_pyarrow_keys_and_values(self):
        got = geoda.group_by(pa.array([1, 2, 1, 2, 1])).sum(pa.array([1, 2, 3, 4, 5]))
        self.assertEqual(got, {"keys": [1, 2], "values": [9, 6]})
        self.assertIs(type(got["keys"][0]), int)
        self.assertIs(type(got["values"][0]), int)

    def test_key_types_come_back_as_python_types(self):
        values = pa.array([1, 2, 3, 4])
        cases = {
            "bool": (pa.array([True, False, True, None]), [True, False, None]),
            "int32": (pa.array([7, 8, 7, None], pa.int32()), [7, 8, None]),
            "float64": (pa.array([1.5, 2.5, 1.5, None]), [1.5, 2.5, None]),
        }
        for name, (keys, expected) in cases.items():
            with self.subTest(name):
                got = geoda.group_by(keys).count(values)
                self.assertEqual(got["keys"], expected)
        got = geoda.group_by(pa.array(["a", "b", "a", None])).count(values)
        self.assertEqual(got, {"keys": ["a", "b", None], "values": [2, 1, 1]})

    def test_empty_string_is_a_group(self):
        got = geoda.group_by(pa.array(["", "x", ""])).sum(pa.array([1.5, 2.0, 3.0]))
        self.assertEqual(got, {"keys": ["", "x"], "values": [4.5, 2.0]})

    def test_text_values_are_type_errors(self):
        with self.assertRaises(TypeError):
            geoda.group_by(pa.array([1, 2])).sum(pa.array(["a", "b"]))

    def test_sliced_columns_use_the_window(self):
        keys = pa.array([9, 1, 1, 2, 9]).slice(1, 3)
        values = pa.array([100, 1, 2, 3, 900]).slice(1, 3)
        self.assertEqual(geoda.group_by(keys).sum(values), {"keys": [1, 2], "values": [3, 3]})


if __name__ == "__main__":
    unittest.main()
