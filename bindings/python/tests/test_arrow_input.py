"""Entrada de columnas Arrow en `geoda.median` y `geoda.percentile` (HU-025).

Se usa `pyarrow` como productor del protocolo Arrow PyCapsule; los objetos genericos
(`Producer`) solo exponen `__arrow_c_array__`, como lo haria polars o pandas. Se salta si
`pyarrow` no esta instalado.
"""

import ctypes
import gc
import unittest

import capi
import geoda

try:
    import pyarrow as pa
except ImportError:  # pragma: no cover - depende del entorno
    pa = None

try:
    import numpy as np
except ImportError:  # pragma: no cover - depende del entorno
    np = None

_get_pointer = ctypes.pythonapi.PyCapsule_GetPointer
_get_pointer.restype = ctypes.c_void_p
_get_pointer.argtypes = [ctypes.py_object, ctypes.c_char_p]


class Producer:
    """Objeto con solo `__arrow_c_array__`, sin ser un array de pyarrow."""

    def __init__(self, array):
        self._array = array

    def __arrow_c_array__(self, requested_schema=None):
        return self._array.__arrow_c_array__(requested_schema)


class CountingProducer(Producer):
    """Como `Producer`, pero cuenta cuantas veces se llama al `release` del array."""

    def __init__(self, array):
        super().__init__(array)
        self.releases = 0
        self._keep = []

    def __arrow_c_array__(self, requested_schema=None):
        schema_capsule, array_capsule = self._array.__arrow_c_array__(requested_schema)
        struct = ctypes.cast(
            _get_pointer(array_capsule, b"arrow_array"), ctypes.POINTER(capi.ArrowArray)
        ).contents
        # Copia la direccion: `struct.release` es una vista de la memoria de la struct y
        # apuntaria al contador despues de asignarlo, llamandose a si mismo sin fin.
        original = capi.RELEASE_ARRAY(ctypes.cast(struct.release, ctypes.c_void_p).value)

        def counting(ptr):
            self.releases += 1
            original(ptr)

        callback = capi.RELEASE_ARRAY(counting)
        self._keep.append(callback)
        struct.release = callback
        return schema_capsule, array_capsule


@unittest.skipIf(pa is None, "pyarrow no esta instalado")
class ArrowInput(unittest.TestCase):
    def test_pyarrow_array_gives_the_same_as_the_list(self):
        self.assertEqual(geoda.median(pa.array([3, 1, None, 2])), 2.0)
        self.assertEqual(geoda.median(pa.array([3, 1, None, 2])), geoda.median([3, 1, None, 2]))
        self.assertEqual(geoda.percentile(pa.array([1, 2, 3, 4]), 25), 1.75)

    def test_any_object_with_the_arrow_protocol_works(self):
        self.assertEqual(geoda.median(Producer(pa.array([5.0, 1.0, 3.0]))), 3.0)
        self.assertEqual(geoda.percentile(Producer(pa.array([1, 2, 3, 4])), 50), 2.5)

    @unittest.skipIf(np is None, "numpy no esta instalado")
    def test_numpy_through_pyarrow(self):
        self.assertEqual(geoda.median(pa.array(np.array([4.0, 1.0, 2.0]))), 2.0)

    def test_every_numeric_type_matches_the_list_with_and_without_nulls(self):
        cases = {
            "int32": ([7, 1, 4, 9], pa.int32()),
            "int32 con nulos": ([7, None, 4, 9, None], pa.int32()),
            "int64": ([7, 1, 4, 9], pa.int64()),
            "int64 con nulos": ([7, None, 4, 9, None], pa.int64()),
            "float64": ([7.5, 1.25, 4.0], pa.float64()),
            "float64 con nulos": ([7.5, None, 1.25, 4.0], pa.float64()),
            "bool": ([True, False, True], pa.bool_()),
            "bool con nulos": ([True, None, False, True, True], pa.bool_()),
        }
        for name, (values, dtype) in cases.items():
            with self.subTest(name):
                arr = pa.array(values, dtype)
                self.assertEqual(geoda.median(arr), geoda.median(values))
                for p in (0, 25, 90, 100):
                    self.assertEqual(geoda.percentile(arr, p), geoda.percentile(values, p))

    def test_sliced_array_uses_the_window_not_the_buffer(self):
        arr = pa.array([100, 1, 2, 3, 900], pa.int64()).slice(1, 3)
        self.assertEqual(geoda.median(arr), 2.0)
        sliced_float = pa.array([1000.0, None, 10.0, 20.0, 30.0]).slice(1, 3)
        self.assertEqual(geoda.median(sliced_float), 15.0)

    def test_unsupported_types_are_clear_errors(self):
        for name, arr in {
            "texto": pa.array(["a", "b"]),
            "decimal128": pa.array([1, 2], pa.decimal128(10, 2)),
            "lista anidada": pa.array([[1], [2, 3]]),
        }.items():
            with self.subTest(name):
                with self.assertRaises((TypeError, ValueError)) as ctx:
                    geoda.median(arr)
                self.assertTrue(str(ctx.exception))

    def test_object_that_is_neither_list_nor_arrow_is_type_error(self):
        with self.assertRaises(TypeError):
            geoda.median(42)

    def test_only_nulls_is_value_error(self):
        with self.assertRaises(ValueError):
            geoda.median(pa.array([None, None], pa.int64()))

    def test_release_is_called_exactly_once_on_success(self):
        producer = CountingProducer(pa.array([3, 1, 2], pa.int64()))
        self.assertEqual(geoda.median(producer), 2.0)
        gc.collect()
        self.assertEqual(producer.releases, 1)

    def test_release_is_called_exactly_once_when_the_call_fails(self):
        for name, arr in {
            "tipo no soportado": pa.array(["a"]),
            "sin valores validos": pa.array([None], pa.int64()),
        }.items():
            with self.subTest(name):
                producer = CountingProducer(arr)
                with self.assertRaises((TypeError, ValueError)):
                    geoda.median(producer)
                gc.collect()
                self.assertEqual(producer.releases, 1)

    def test_release_is_called_exactly_once_when_p_is_invalid(self):
        producer = CountingProducer(pa.array([3, 1, 2], pa.int64()))
        with self.assertRaises(ValueError):
            geoda.percentile(producer, 101)
        gc.collect()
        self.assertEqual(producer.releases, 1)

    def test_reusing_a_consumed_capsule_is_an_error_not_a_crash(self):
        class Reuse:
            def __init__(self):
                self.pair = pa.array([3, 1, 2]).__arrow_c_array__()

            def __arrow_c_array__(self, requested_schema=None):
                return self.pair

        reuse = Reuse()
        self.assertEqual(geoda.median(reuse), 2.0)
        with self.assertRaises(ValueError):
            geoda.median(reuse)

    def test_protocol_returning_non_capsules_is_type_error(self):
        class Bad:
            def __arrow_c_array__(self, requested_schema=None):
                return (1, 2)

        with self.assertRaises(TypeError):
            geoda.median(Bad())


if __name__ == "__main__":
    unittest.main()
