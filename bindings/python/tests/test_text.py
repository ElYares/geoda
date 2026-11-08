"""Pruebas del texto de la fase 5: `edit_distance`, `lis` y Huffman (`huffman_encode` y
`huffman_decode`), y los casos de `spec/cases/text/` que se pueden ejecutar por el binding.
"""

import json
import unittest
from pathlib import Path

import geoda

try:
    import pyarrow as pa
except ImportError:
    pa = None

CASES = sorted((Path(__file__).resolve().parents[3] / "spec" / "cases" / "text").glob("*.json"))
EXCEPTIONS = {
    "INVALID_ARGUMENT": ValueError,
    "LENGTH_MISMATCH": ValueError,
    "TYPE_MISMATCH": TypeError,
    "UNSUPPORTED": TypeError,
}


def _values(column):
    """Valores de un caso; en `float64`, `"NaN"` y compañia pasan a `float`."""
    if column["dtype"] != "float64":
        return column["values"]
    return [float(v) if isinstance(v, str) else v for v in column["values"]]


class EditDistance(unittest.TestCase):
    def test_two_strings_give_an_int(self):
        self.assertEqual(geoda.edit_distance("kitten", "sitting"), 3)
        self.assertEqual(geoda.edit_distance("", ""), 0)
        self.assertIsInstance(geoda.edit_distance("a", "b"), int)

    def test_counts_characters_not_bytes(self):
        self.assertEqual(geoda.edit_distance("añb", "anb"), 1)

    def test_one_string_against_a_list(self):
        self.assertEqual(geoda.edit_distance(["casa", "cosa", None], "casa"), [0, 1, None])
        self.assertEqual(geoda.edit_distance("casa", ["casa", "cosa"]), [0, 1])

    def test_lists_of_the_same_length(self):
        self.assertEqual(geoda.edit_distance(["a", None, "abc"], ["b", "x", "abd"]), [1, None, 1])

    def test_lengths_that_do_not_match(self):
        with self.assertRaises(ValueError):
            geoda.edit_distance(["a", "b"], ["a", "b", "c"])

    def test_not_text(self):
        with self.assertRaises(TypeError):
            geoda.edit_distance(["a", "b"], [1, 2])

    @unittest.skipIf(pa is None, "pyarrow no esta instalado")
    def test_arrow_utf8(self):
        self.assertEqual(geoda.edit_distance(pa.array(["a", "ab", None]), "b"), [1, 1, None])

    @unittest.skipIf(pa is None, "pyarrow no esta instalado")
    def test_arrow_not_text(self):
        with self.assertRaises(TypeError):
            geoda.edit_distance(pa.array([1, 2]), "a")


class Lis(unittest.TestCase):
    def test_classic_case(self):
        result = geoda.lis([10, 9, 2, 5, 3, 7, 101, 18])
        self.assertEqual(result, {"length": 4, "indices": [2, 4, 5, 7]})

    def test_empty_and_decreasing(self):
        self.assertEqual(geoda.lis([]), {"length": 0, "indices": []})
        self.assertEqual(geoda.lis([5, 4, 3])["length"], 1)

    def test_equal_values_do_not_chain(self):
        self.assertEqual(geoda.lis([2, 2, 2])["length"], 1)

    def test_none_and_nan_are_value_errors(self):
        with self.assertRaises(ValueError):
            geoda.lis([1, None, 3])
        with self.assertRaises(ValueError):
            geoda.lis([1.0, float("nan")])

    @unittest.skipIf(pa is None, "pyarrow no esta instalado")
    def test_arrow_and_text(self):
        self.assertEqual(geoda.lis(pa.array([3, 1, 2], pa.int32()))["indices"], [1, 2])
        with self.assertRaises(TypeError):
            geoda.lis(pa.array(["a", "b"]))


class Huffman(unittest.TestCase):
    def test_round_trip_of_text_and_bytes(self):
        for data in ("abracadabra", b"\x00\x01\x02\x00\x00", bytearray(b"zzz"), ""):
            encoded = geoda.huffman_encode(data)
            self.assertEqual(len(encoded["frequencies"]), 256)
            self.assertEqual(len(encoded["bits"]), -(-encoded["bit_len"] // 8))
            decoded = geoda.huffman_decode(
                encoded["bits"], encoded["bit_len"], encoded["frequencies"]
            )
            expected = data.encode() if isinstance(data, str) else bytes(data)
            self.assertEqual(decoded, expected)

    def test_it_compresses_skewed_data(self):
        encoded = geoda.huffman_encode("a" * 900 + "b" * 90 + "c" * 10)
        self.assertLess(encoded["bit_len"], 1000 * 8 // 4)

    def test_known_code(self):
        # aaabbc: a=0, c=10, b=11 -> 000 11 11 10 = 9 bits.
        encoded = geoda.huffman_encode(b"aaabbc")
        self.assertEqual(encoded["bit_len"], 9)
        self.assertEqual(encoded["bits"], bytes([0b0001_1111, 0b0000_0000]))

    def test_decode_errors(self):
        frequencies = geoda.huffman_encode(b"aaabbc")["frequencies"]
        with self.assertRaises(ValueError):
            geoda.huffman_decode(b"\x40", 2, frequencies)  # termina a media palabra
        with self.assertRaises(ValueError):
            geoda.huffman_decode(b"\x00", 9, frequencies)  # mas bits de los que hay
        with self.assertRaises(ValueError):
            geoda.huffman_decode(b"\x00", 1, [1, 2, 3])  # no son 256 frecuencias
        with self.assertRaises(ValueError):
            geoda.huffman_decode(b"\x00", 1, [-1] + [0] * 255)


class SpecCases(unittest.TestCase):
    """Los casos `edit_distance` y `lis` de la spec dan lo mismo por el binding."""

    def test_cases(self):
        ran = 0
        for path in CASES:
            case = json.loads(path.read_text(encoding="utf-8"))
            op = case["op"]
            if op not in ("edit_distance", "lis"):
                continue
            with self.subTest(case=case["id"]):
                ran += 1
                if op == "edit_distance":
                    call = lambda: geoda.edit_distance(  # noqa: E731
                        case["input"]["values"], case["input2"]["values"]
                    )
                else:
                    call = lambda: geoda.lis(_values(case["input"]))  # noqa: E731
                error = case["expect"].get("error")
                if error is not None:
                    with self.assertRaises(EXCEPTIONS[error]):
                        call()
                    continue
                result = call()
                want = case["expect"]["columns"][0]["values"]
                self.assertEqual(result if op == "edit_distance" else result["indices"], want)
        self.assertGreaterEqual(ran, 8)


if __name__ == "__main__":
    unittest.main()
