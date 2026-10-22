"""Acceso a la C-API de geoda (`libgeoda`) por `ctypes`, solo con la biblioteca estandar.

Es lo que usa el runner de `spec/` y el test de interop: ejercita la misma frontera
que veria cualquier otro binding. Las estructuras de Arrow se declaran aqui a mano.
"""

import ctypes
import os
import struct
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]

# Codigos de `geoda.h`; el nombre es el sufijo de `GEODA_ERR_*` (ver spec/README.md).
OK = 0
CATEGORIES = {
    -1: "INVALID_ARGUMENT",
    -2: "TYPE_MISMATCH",
    -3: "LENGTH_MISMATCH",
    -4: "OUT_OF_BOUNDS",
    -5: "UNSUPPORTED",
    -6: "ALLOC",
    -7: "FFI",
    -8: "PANIC",
    -9: "INTERNAL",
}


def category(code):
    return CATEGORIES.get(code, "DESCONOCIDO")


class ArrowSchema(ctypes.Structure):
    pass


class ArrowArray(ctypes.Structure):
    pass


RELEASE_SCHEMA = ctypes.CFUNCTYPE(None, ctypes.POINTER(ArrowSchema))
RELEASE_ARRAY = ctypes.CFUNCTYPE(None, ctypes.POINTER(ArrowArray))

ArrowSchema._fields_ = [
    ("format", ctypes.c_char_p),
    ("name", ctypes.c_char_p),
    ("metadata", ctypes.c_char_p),
    ("flags", ctypes.c_int64),
    ("n_children", ctypes.c_int64),
    ("children", ctypes.POINTER(ctypes.POINTER(ArrowSchema))),
    ("dictionary", ctypes.POINTER(ArrowSchema)),
    ("release", RELEASE_SCHEMA),
    ("private_data", ctypes.c_void_p),
]

ArrowArray._fields_ = [
    ("length", ctypes.c_int64),
    ("null_count", ctypes.c_int64),
    ("offset", ctypes.c_int64),
    ("n_buffers", ctypes.c_int64),
    ("n_children", ctypes.c_int64),
    ("buffers", ctypes.POINTER(ctypes.c_void_p)),
    ("children", ctypes.POINTER(ctypes.POINTER(ArrowArray))),
    ("dictionary", ctypes.POINTER(ArrowArray)),
    ("release", RELEASE_ARRAY),
    ("private_data", ctypes.c_void_p),
]


def _library_path():
    """Ruta de `libgeoda`: `GEODA_LIB` si esta definida, si no `target/{debug,release}`."""
    if "GEODA_LIB" in os.environ:
        return Path(os.environ["GEODA_LIB"])
    names = {"win32": "geoda.dll", "darwin": "libgeoda.dylib"}.get(sys.platform, "libgeoda.so")
    for profile in ("debug", "release"):
        candidate = ROOT / "target" / profile / names
        if candidate.exists():
            return candidate
    raise FileNotFoundError(
        "no encuentro libgeoda: corre `cargo build -p geoda-capi` o define GEODA_LIB"
    )


def _load():
    lib = ctypes.CDLL(str(_library_path()))
    p_schema, p_array = ctypes.POINTER(ArrowSchema), ctypes.POINTER(ArrowArray)
    p_handle = ctypes.c_void_p
    signatures = {
        "geoda_abi_version": ([], ctypes.c_uint32),
        "geoda_last_error_message": ([], ctypes.c_char_p),
        "geoda_column_from_arrow": ([p_schema, p_array, ctypes.POINTER(p_handle)], ctypes.c_int32),
        "geoda_column_to_arrow": ([p_handle, p_schema, p_array], ctypes.c_int32),
        "geoda_column_slice": (
            [p_handle, ctypes.c_size_t, ctypes.c_size_t, ctypes.POINTER(p_handle)],
            ctypes.c_int32,
        ),
        "geoda_column_len": ([p_handle], ctypes.c_int64),
        "geoda_column_null_count": ([p_handle], ctypes.c_int64),
        "geoda_column_median": ([p_handle, ctypes.POINTER(ctypes.c_double)], ctypes.c_int32),
        "geoda_column_percentile": (
            [p_handle, ctypes.c_double, ctypes.POINTER(ctypes.c_double)],
            ctypes.c_int32,
        ),
        "geoda_column_free": ([p_handle], None),
    }
    for name, (argtypes, restype) in signatures.items():
        fn = getattr(lib, name)
        fn.argtypes, fn.restype = argtypes, restype
    return lib


LIB = _load()


def abi_version():
    return LIB.geoda_abi_version()


def last_error():
    return (LIB.geoda_last_error_message() or b"").decode()


class Column:
    """Handle de la capi. Se libera con `close()` o al salir del `with`."""

    def __init__(self, handle):
        self.handle = ctypes.c_void_p(handle)

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        self.close()

    def close(self):
        if self.handle.value:
            LIB.geoda_column_free(self.handle)
            self.handle = ctypes.c_void_p(None)

    def __len__(self):
        return LIB.geoda_column_len(self.handle)

    @property
    def null_count(self):
        return LIB.geoda_column_null_count(self.handle)

    def slice(self, offset, length):
        """`(Column, 0)` si sale bien; `(None, codigo)` si la capi rechaza el rango."""
        out = ctypes.c_void_p(None)
        code = LIB.geoda_column_slice(self.handle, offset, length, ctypes.byref(out))
        return (Column(out.value), code) if code == OK else (None, code)

    def median(self):
        """`(valor, 0)` si sale bien; `(None, codigo)` si la capi falla."""
        out = ctypes.c_double(0.0)
        code = LIB.geoda_column_median(self.handle, ctypes.byref(out))
        return (out.value, code) if code == OK else (None, code)

    def percentile(self, p):
        """`(valor, 0)` si sale bien; `(None, codigo)` si la capi falla."""
        out = ctypes.c_double(0.0)
        code = LIB.geoda_column_percentile(self.handle, p, ctypes.byref(out))
        return (out.value, code) if code == OK else (None, code)

    def export(self):
        """Exporta por `geoda_column_to_arrow`. Devuelve `(schema, array, codigo)`.

        Quien llama debe liberar con `release_exported`.
        """
        schema, array = ArrowSchema(), ArrowArray()
        code = LIB.geoda_column_to_arrow(self.handle, ctypes.byref(schema), ctypes.byref(array))
        return schema, array, code


def from_arrow(schema, array):
    """`(Column, 0)` si la capi importa; `(None, codigo)` si no. Con exito consume `array`."""
    out = ctypes.c_void_p(None)
    code = LIB.geoda_column_from_arrow(ctypes.byref(schema), ctypes.byref(array), ctypes.byref(out))
    return (Column(out.value), code) if code == OK else (None, code)


def release_exported(schema, array):
    """Llama a `release` de las structs si siguen presentes (una vez cada una)."""
    if array.release:
        array.release(ctypes.byref(array))
    if schema.release:
        schema.release(ctypes.byref(schema))


# --- Construccion de Arrow a mano (productor minimo) ---------------------------------

# Los callbacks `release` y los buffers que deben seguir vivos mientras C pueda usarlos.
# Si se pierde la referencia, el proceso muere con un segfault.
_KEEP = {}
_COUNTERS = {"array_releases": 0, "schema_releases": 0}


@RELEASE_ARRAY
def _release_array(ptr):
    _KEEP.pop(("array", ctypes.addressof(ptr.contents)), None)
    ptr.contents.release = ctypes.cast(None, RELEASE_ARRAY)
    _COUNTERS["array_releases"] += 1


@RELEASE_SCHEMA
def _release_schema(ptr):
    _KEEP.pop(("schema", ctypes.addressof(ptr.contents)), None)
    ptr.contents.release = ctypes.cast(None, RELEASE_SCHEMA)
    _COUNTERS["schema_releases"] += 1


def array_releases():
    return _COUNTERS["array_releases"]


def parse_float(value):
    """Flotante del JSON: numero, o una de las cuatro cadenas especiales."""
    if isinstance(value, str):
        specials = {"NaN": float("nan"), "Infinity": float("inf"), "-Infinity": float("-inf"), "-0.0": -0.0}
        if value not in specials:
            raise ValueError(f"flotante especial desconocido: {value!r}")
        return specials[value]
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise ValueError(f"no es un flotante: {value!r}")
    return float(value)


def same_float(a, b):
    """Mismos bits, o ambos NaN."""
    if a != a and b != b:
        return True
    return struct.pack("<d", a) == struct.pack("<d", b)


_FORMATS = {"bool": b"b", "int32": b"i", "int64": b"l", "float64": b"g", "utf8": b"u"}


def _validity(values):
    if all(v is not None for v in values):
        return None
    bits = bytearray((len(values) + 7) // 8)
    for i, v in enumerate(values):
        if v is not None:
            bits[i // 8] |= 1 << (i % 8)
    return (ctypes.c_uint8 * max(len(bits), 1)).from_buffer_copy(bytes(bits) or b"\0")


def build_arrow(dtype, values):
    """Arma `(ArrowSchema, ArrowArray)` para `values` con memoria propia de Python.

    `decimal128` es el tipo de prueba negativa: sobre datos Int32 pero con el formato
    Arrow `d:10,2`, que geoda aun no soporta.
    """
    n = len(values)
    keep = []
    validity = _validity(values)
    keep.append(validity)

    if dtype == "bool":
        bits = bytearray((n + 7) // 8)
        for i, v in enumerate(values):
            if v:
                bits[i // 8] |= 1 << (i % 8)
        data = [(ctypes.c_uint8 * max(len(bits), 1)).from_buffer_copy(bytes(bits) or b"\0")]
    elif dtype in ("int32", "decimal128"):
        data = [(ctypes.c_int32 * n)(*[0 if v is None else v for v in values])]
    elif dtype == "int64":
        data = [(ctypes.c_int64 * n)(*[0 if v is None else v for v in values])]
    elif dtype == "float64":
        data = [(ctypes.c_double * n)(*[0.0 if v is None else parse_float(v) for v in values])]
    elif dtype == "utf8":
        offsets, chunks, total = [0], [], 0
        for v in values:
            raw = b"" if v is None else v.encode("utf-8")
            chunks.append(raw)
            total += len(raw)
            offsets.append(total)
        payload = b"".join(chunks)
        data = [
            (ctypes.c_int32 * (n + 1))(*offsets),
            ctypes.create_string_buffer(payload or b"\0", max(len(payload), 1)),
        ]
    else:
        raise ValueError(f"dtype desconocido: {dtype!r}")
    keep.extend(data)

    pointers = (ctypes.c_void_p * (1 + len(data)))(
        ctypes.cast(validity, ctypes.c_void_p) if validity is not None else None,
        *[ctypes.cast(b, ctypes.c_void_p) for b in data],
    )
    keep.append(pointers)

    array = ArrowArray()
    array.length = n
    array.null_count = sum(1 for v in values if v is None)
    array.offset = 0
    array.n_buffers = len(pointers)
    array.buffers = ctypes.cast(pointers, ctypes.POINTER(ctypes.c_void_p))
    array.release = _release_array
    _KEEP[("array", ctypes.addressof(array))] = keep

    schema = ArrowSchema()
    schema.format = b"d:10,2" if dtype == "decimal128" else _FORMATS[dtype]
    schema.name = b"x"
    schema.flags = 2  # ARROW_FLAG_NULLABLE
    schema.release = _release_schema
    _KEEP[("schema", ctypes.addressof(schema))] = [schema.format, schema.name]
    return schema, array


# --- Lectura de Arrow exportado por geoda --------------------------------------------


def _bit(buffer_address, index):
    return (ctypes.c_uint8.from_address(buffer_address + index // 8).value >> (index % 8)) & 1


def read_arrow(schema, array):
    """Decodifica una columna hoja exportada: `(dtype, valores)`; `None` = nulo.

    Respeta `offset`. Los flotantes se devuelven como `float`.
    """
    fmt = schema.format
    names = {v: k for k, v in _FORMATS.items()}
    if fmt not in names:
        raise ValueError(f"formato Arrow no soportado por el lector: {fmt!r}")
    dtype = names[fmt]
    n, off = array.length, array.offset
    validity = array.buffers[0]

    def is_null(i):
        return bool(validity) and not _bit(validity, off + i)

    values = []
    for i in range(n):
        if is_null(i):
            values.append(None)
        elif dtype == "bool":
            values.append(bool(_bit(array.buffers[1], off + i)))
        elif dtype == "int32":
            values.append(ctypes.c_int32.from_address(array.buffers[1] + 4 * (off + i)).value)
        elif dtype == "int64":
            values.append(ctypes.c_int64.from_address(array.buffers[1] + 8 * (off + i)).value)
        elif dtype == "float64":
            values.append(ctypes.c_double.from_address(array.buffers[1] + 8 * (off + i)).value)
        else:  # utf8
            start = ctypes.c_int32.from_address(array.buffers[1] + 4 * (off + i)).value
            end = ctypes.c_int32.from_address(array.buffers[1] + 4 * (off + i + 1)).value
            values.append(ctypes.string_at(array.buffers[2] + start, end - start).decode("utf-8"))
    return dtype, values
