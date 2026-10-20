"""Interop con pyarrow como oraculo: una implementacion de Arrow independiente de geoda.

Cada tipo pasa pyarrow -> `geoda_column_from_arrow` -> `geoda_column_to_arrow` ->
pyarrow y debe volver igual, y tambien en el sentido contrario (una columna que geoda
exporta debe poder leerla pyarrow). Se salta si `pyarrow` no esta instalado.
"""

import ctypes
import unittest

import capi

try:
    import pyarrow as pa
except ImportError:  # pragma: no cover - depende del entorno
    pa = None


def _addr(struct):
    return ctypes.addressof(struct)


@unittest.skipIf(pa is None, "pyarrow no esta instalado")
class PyarrowInterop(unittest.TestCase):
    CASES = {
        "bool": pa.array([True, None, False, True], pa.bool_()) if pa else None,
        "int32": pa.array([1, None, -3, 2**31 - 1], pa.int32()) if pa else None,
        "int64": pa.array([1, None, -(2**63), 2**63 - 1], pa.int64()) if pa else None,
        "float64": pa.array([1.5, None, float("inf"), -0.0], pa.float64()) if pa else None,
        "utf8": pa.array(["", None, "ñandú", "hola"], pa.string()) if pa else None,
        "sin nulos": pa.array([1, 2, 3], pa.int64()) if pa else None,
        "vacia": pa.array([], pa.int64()) if pa else None,
        "utf8 vacia": pa.array([], pa.string()) if pa else None,
        "con offset": pa.array([10, None, 30, 40, None], pa.int32()).slice(1, 3) if pa else None,
        "utf8 con offset": pa.array(["a", "bb", None, "dddd"], pa.string()).slice(1, 3) if pa else None,
    }

    @staticmethod
    def _into_geoda(arr):
        schema, array = capi.ArrowSchema(), capi.ArrowArray()
        arr._export_to_c(_addr(array), _addr(schema))
        column, code = capi.from_arrow(schema, array)
        capi.release_exported(schema, array)  # el esquema es nuestro; el array ya se consumio
        return column, code

    @staticmethod
    def _into_pyarrow(column):
        schema, array, code = column.export()
        assert code == capi.OK, capi.last_error()
        # `_import_from_c` toma propiedad de ambas structs y las libera cuando se suelta.
        return pa.Array._import_from_c(_addr(array), _addr(schema))

    def test_pyarrow_to_geoda_and_back(self):
        for name, arr in self.CASES.items():
            with self.subTest(case=name):
                column, code = self._into_geoda(arr)
                self.assertEqual(code, capi.OK, capi.last_error())
                with column:
                    self.assertEqual(len(column), len(arr))
                    self.assertEqual(column.null_count, arr.null_count)
                    back = self._into_pyarrow(column)
                self.assertTrue(back.equals(arr), f"{back} != {arr}")
                self.assertEqual(back.type, arr.type)

    def test_geoda_slice_is_readable_by_pyarrow(self):
        column, code = self._into_geoda(pa.array([10, None, 30, 40, None], pa.int64()))
        self.assertEqual(code, capi.OK)
        with column:
            part, code = column.slice(1, 3)
            self.assertEqual(code, capi.OK)
            with part:
                back = self._into_pyarrow(part)
        self.assertEqual(back.to_pylist(), [None, 30, 40])

    def test_export_outlives_the_handle(self):
        column, code = self._into_geoda(pa.array(["x", None, "zz"], pa.string()))
        self.assertEqual(code, capi.OK)
        with column:
            schema, array, code = column.export()
        self.assertEqual(code, capi.OK)
        back = pa.Array._import_from_c(_addr(array), _addr(schema))  # el handle ya se libero
        self.assertEqual(back.to_pylist(), ["x", None, "zz"])

    def test_unsupported_type_is_reported_not_crashed(self):
        column, code = self._into_geoda(pa.array([1, 2], pa.int8()))
        self.assertIsNone(column)
        self.assertEqual(capi.category(code), "UNSUPPORTED")
        self.assertNotEqual(capi.last_error(), "")


if __name__ == "__main__":
    unittest.main()
