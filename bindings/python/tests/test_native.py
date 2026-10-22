"""Pruebas del modulo nativo `geoda` (requiere `maturin develop`)."""

import math
import unittest

import geoda


class Statistics(unittest.TestCase):
    def test_median_ignores_nulls(self):
        self.assertEqual(geoda.median([3, 1, None, 2]), 2.0)
        self.assertEqual(geoda.median([1, 2, 3, 4]), 2.5)

    def test_percentile_is_linear_type_7(self):
        self.assertEqual(geoda.percentile([1, 2, 3, 4], 25), 1.75)
        self.assertEqual(geoda.percentile([1.5, 9.0], 0), 1.5)
        self.assertEqual(geoda.percentile([1.5, 9.0], 100), 9.0)

    def test_nan_propagates(self):
        self.assertTrue(math.isnan(geoda.median([1.0, float("nan"), 3.0])))

    def test_invalid_input_is_value_error(self):
        for call in (
            lambda: geoda.median([]),
            lambda: geoda.median([None, None]),
            lambda: geoda.percentile([1, 2, 3], 100.5),
            lambda: geoda.percentile([1, 2, 3], -1),
            lambda: geoda.percentile([1, 2, 3], float("nan")),
        ):
            with self.subTest(call=call):
                with self.assertRaises(ValueError):
                    call()


if __name__ == "__main__":
    unittest.main()
